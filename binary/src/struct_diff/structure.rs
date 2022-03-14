use regex::Regex;
use super::{divider::*, diff::*, range_vec::*};
use rayon::prelude::*;
use crate::utils::*;
use std::ops::RangeInclusive;

#[derive(Debug, Clone)]
pub struct Key {
    pub fuzzed: Option<Regex>,
    pub strict: Regex,
}

impl Key {
    fn find<'a, S>(&'a self, line: S) -> Vec<String>
    where
        S: AsRef<str> + 'a
    {
        let strict_find = |fuzzed: regex::Matches<'a, 'a>| fuzzed
            .flat_map(|token| self
                .strict
                .find_iter(token.as_str())
                .map(|found| found.as_str())
                .flat_map(|found| (!found.is_empty()).then(|| found.to_string()))
            );

        self.fuzzed
            .as_ref()
            .map(|regex| regex.find_iter(line.as_ref()))
            .map_or(vec![], |matches| strict_find(matches).collect())
    }
}

#[derive(Debug, Clone)]
pub struct Config {
    pub filter: Option<Divider>,
    pub expander: Option<Divider>,
    pub keys: Vec<Key>,
}

//  Can't alias the constraints because I would need GATs
//  https://github.com/rust-lang/rfcs/pull/1598
impl Config {
    pub fn filtered<I, S>(&self, iter: I) -> RangeVec 
    where
        I: Iterator<Item = S> + Clone + Send + Sync,
        S: AsRef<str> + Send
    {
        self.filter
            .as_ref()
            .map(|filter| filter.divide(iter.clone()))
            .unwrap_or_else(|| [(0, iter.clone().count())].into())
    }

    pub fn objs<I, S>(&self, iter: I) -> RangeVec 
    where
        I: Iterator<Item = S> + Clone + Send + Sync,
        S: AsRef<str> + Send
    {
        self.expander
            .as_ref()
            .map(|expander| expander.divide(iter.clone()))
            .unwrap_or_else(|| (0..iter.clone().count()).into())
            .intersection_with(self.filtered(iter))
    }
}

#[derive(Debug, Clone)]
pub struct Structure {
    pub contents: Vec<String>,
    pub config: Config,
}


//  Todo:
//  
//      Add errors for multiple key matches/no key matches
//
impl Structure {
    fn new(text: String, config: Config) -> Structure {
        Structure {
            contents: text
                .lines()
                .map(|line| line.to_string())
                .collect::<Vec<_>>(),
            config,
        }
    }

    fn keys(&self) -> Vec<(usize, Vec<String>)> {
        let to_keys = |span: RangeInclusive<usize>| span
            .clone()
            .zip(self.contents[span].iter())
            .map(|(i, line)| (i, self
                    .config
                    .keys
                    .iter()
                    .flat_map(|key| key.find(line.clone()).into_iter())
                    .collect::<Vec<_>>()
                )
            );

        self.config
            .objs(self.contents.iter())
            .par_iter()
            .map(|range| range.lower..=range.upper)
            .flat_map(|range| to_keys(range).collect::<Vec<_>>().into_par_iter())
            .collect::<Vec<_>>()
    }

    fn inflate(config: &Config, patched: &[String], indices: &[usize]) -> Structure {
        let mut indices: RangeVec = indices.to_vec().into();

        indices.dedup();

        let objs = config
            .objs(patched.iter())
            .that_overlap(indices);

        let inflated = config
            .filtered(patched.iter())
            .inverse(patched.len())
            .union_with(objs)
            .pre_ops()
            .iter()
            .flat_map(|r| &patched[r.lower..=r.upper])
            .cloned()
            .collect::<Vec<_>>();

        Self {
            contents: inflated,
            config: config.clone()
        }
    }

    pub fn forward_inflate(&self, modifications: &StructDiff) -> Structure {
        let remade = modifications.patch(self.contents.iter());
        Self::inflate(&self.config, &remade, &modifications.added)
    }

    // Todo: Make exclusive?
    pub fn backward_inflate(&self, modifications: &StructDiff) -> Structure {
        Self::inflate(&self.config, &self.contents, &modifications.removed)
    }

    pub fn conflicts(&self, left: &StructDiff, right: &StructDiff) -> Option<(Structure, Structure)> {
        let left = self.forward_inflate(left);
        let right = self.forward_inflate(right);
 
        let overlapping = |((i, left), (j, right)): ((usize, Vec<String>), (usize, Vec<String>))| {
            let mut same = left
                .iter()
                .zip(right.iter())  //Todo: Handle size mismatch
                .filter(|(a, b)| a == b)
                .peekable();

            same.peek()
                .is_some()
                .then(|| (i, j))
        };

        let mut collisions = left
            .keys()
            .into_iter()
            .zip(right.keys().into_iter())
            .filter_map(overlapping)
            .peekable();

        collisions.peek().is_some().then(|| {
            let keep_left = Self::inflate(
                &self.config, 
                &left.contents, 
                &collisions
                    .clone()
                    .map(|(i, _)| i)
                    .collect::<Vec<_>>()
            );

            let keep_right = Self::inflate(
                &self.config,
                &right.contents,
                &collisions
                    .map(|(_, j)| j)
                    .collect::<Vec<_>>()
            );

            (keep_left, keep_right)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indoc::{indoc, formatdoc};

    const ORIGINAL: &str = indoc!(
        "config: {
            scroll_speed: 0.1,
            view_distance: 10,
        },
        objs: {
            press: {
                lane: 0,
                time: 0,
                color: 0xFF0000,
            },
            hold: {
                lane: 1,
                time: 0,
                color: 0x00FF00,
            },
            press: {
                lane: 2,
                time: 0,
                color: 0x0000FF,
            },
        }"
    );

    const MODDED_A: &str = indoc!(
        "config: {
            scroll_speed: 0.1,
            view_distance: 10,
        },
        objs: {
            hold: {
                lane: 1,
                time: 4,
                color: 0x80FF00,
            },
            press: {
                lane: 2,
                time: 0,
                color: 0x0000FF,
            },
        },
        misc: {
            extra: 20
        }"
    );

    const MODDED_B: &str = indoc!(
        "config: {
            scroll_speed: 0.1,
            view_distance: 10,
        },
        objs: {
            hold: {
                lane: 1,
                time: 4,
                color: 0x00FF00,
            },
        },"
    );

    fn config() -> Config {
        let config_json = indoc!(r#"
            {
                "keys": [
                    {
                        "fuzzed": "lane: [0-9]*",
                        "strict": "[0-9]*"
                    },
                    {
                        "fuzzed": "time: [0-9]*",
                        "strict": "[0-9]*"
                    }
                ],
                "filter": {
                    "prefix": "objs:",
                    "open": "\\{",
                    "close": "\\}"
                },
                "expander": {
                    "prefix": "(press|hold): \\{",
                    "open": "\\{",
                    "close": "\\}"
                }
            }
        "#);

        serde_json::from_str::<ConfigDef>(config_json)
            .unwrap()
            .into()
    }

    #[test]
    fn key_find() {
        assert_eq!(config().keys[0].find("        lane: 2,"), vec!["2"])
    }

    fn map_keys() {
        let map = Structure::new(ORIGINAL.to_string(), config());
        let mut keys = map.keys();
        keys.sort();

        let line_nums = keys.iter().map(|(i, _)| *i).collect::<Vec<_>>();
        let keys = keys.iter().cloned().map(|(_, k)| k).collect::<Vec<_>>();
        assert_eq!(line_nums, Vec::<usize>::from([6, 11, 16]));
        assert_eq!(keys, vec![vec!["0"], vec!["1"], vec!["2"]]);
    }

    #[test]
    fn config_filtered() {
        let map = Structure::new(ORIGINAL.to_string(), config());
        let filtered = map.config.filtered(ORIGINAL.lines());
        assert_eq!(filtered, [(4, 20)].into());
    }

    #[test]
    fn config_objs() {
        let map = Structure::new(ORIGINAL.to_string(), config());
        let objs = map.config.objs(ORIGINAL.lines());
        assert_eq!(objs, [(5, 9), (10, 14), (15, 19)].into());
    }

    #[test]
    fn forward_inflate() {
        let inflated = Structure::new(ORIGINAL.to_string(), config())
            .forward_inflate(&StructDiff::build_from(ORIGINAL, MODDED_A, ""))
            .contents;

        let expected_inflated = {
            let text = indoc!(
                "config: {
                    scroll_speed: 0.1,
                    view_distance: 10,
                },
                objs: {
                    hold: {
                        lane: 1,
                        time: 4,
                        color: 0x80FF00,
                    },
                },
                misc: {
                    extra: 20
                }"
            );

            text.lines()
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
        };

        assert_eq!(inflated, expected_inflated);
    }

    #[test]
    fn backward_inflate() {
        let inflated = Structure::new(ORIGINAL.to_string(), config())
            .backward_inflate(&StructDiff::build_from(ORIGINAL, MODDED_A, ""))
            .contents;

        let expected_inflated = {
            let text = indoc!(
                "config: {
                    scroll_speed: 0.1,
                    view_distance: 10,
                },
                objs: {
                    press: {
                        lane: 0,
                        time: 0,
                        color: 0xFF0000,
                    },
                    hold: {
                        lane: 1,
                        time: 0,
                        color: 0x00FF00,
                    },
                }"
            );

            text.lines()
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
        };

        assert_eq!(inflated, expected_inflated);
    }

    #[test]
    fn conflicts() {
        let map = Structure::new(ORIGINAL.to_string(), config());
        let modded_a = StructDiff::build_from(ORIGINAL, MODDED_A, "");
        let modded_b = StructDiff::build_from(ORIGINAL, MODDED_B, "");
        let conflicts = map.conflicts(&modded_a, &modded_b);
        assert!(matches!(conflicts, Some(_)));

        let a_conflicts = {
            let text = indoc!(
                "config: {
                    scroll_speed: 0.1,
                    view_distance: 10,
                },
                objs: {
                    hold: {
                        lane: 1,
                        time: 4,
                        color: 0x80FF00,
                    },
                },
                misc: {
                    extra: 20
                }"
            );

            text.lines()
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
        };


        let b_conflicts = {
            let text = indoc!(
                "config: {
                    scroll_speed: 0.1,
                    view_distance: 10,
                },
                objs: {
                    hold: {
                        lane: 1,
                        time: 4,
                        color: 0x00FF00,
                    },
                },"
            );

            text.lines()
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
        };

        let conflicts = conflicts.unwrap();

        assert_eq!(conflicts.0.contents, a_conflicts);
        assert_eq!(conflicts.1.contents, b_conflicts);
    }
}
