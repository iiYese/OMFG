use super::range_vec::InclRange;
use super::range_vec::*;
use rayon::prelude::*;
use regex::Regex;

#[derive(Debug, Clone)]
pub enum Divider {
    Delimited {
        prefix: Regex,
        open: Regex,
        close: Regex,
    },
    Headings {
        fuzzed: Regex,
        strict: Option<Regex>,
        indent: String,
    },
    Enclosures {
        top: Regex,
        bottom: Regex,
    },
}

//  Todo:
//      - Docmuent this exhaustively
//      - Test for edge cases
//      - Add error reporting
//
impl Divider {
    fn balancer<'a, I, S>(&'a self, iter: I) -> Box<dyn Fn(&usize) -> usize + 'a>
    where
        I: Iterator<Item = S> + Clone + 'a,
        S: AsRef<str> 
    {
        match self {
            | Self::Delimited { open, close, .. }
            | Self::Enclosures { top: open, bottom: close } => {
                Box::new(move |&start: &usize| {
                    let mut depth: isize = 0;
                    let iter = iter.clone().enumerate();

                    iter.clone()
                        .skip(start)
                        .skip_while(|(_, line)| !open.is_match(line.as_ref())) // for patterns with prefixes followed by delimiters on a newline
                        .find_map(|(index, line)| {
                            let first = open.find_iter(line.as_ref()).count() as isize;
                            let second = close.find_iter(line.as_ref()).count() as isize;
                            depth += first - second;
                            (depth <= 0).then(|| index)
                        })
                        .unwrap_or_else(|| iter.count() - 1)
                })
            }
            Self::Headings { fuzzed, indent, .. } => {
                let indent_depth = move |line: &str| {
                    indent.is_empty().then(|| 0).unwrap_or_else(|| {
                        line.as_bytes()
                            .windows(indent.len())
                            .step_by(indent.len())
                            .take_while(|&contents| *contents == *indent.as_bytes())
                            .count()
                    })
                };

                Box::new(move |&start: &usize| -> usize {
                    let depth = iter
                        .clone()
                        .nth(start)
                        .map_or(iter.clone().count(), |s| indent_depth(s.as_ref()));

                    iter.clone()
                        .enumerate()
                        .skip(start + 1)
                        .find_map(|(index, line)| {
                            let line = line.as_ref();
                            (fuzzed.is_match(line) && indent_depth(line) == depth).then(|| index - 1)
                        })
                        .unwrap_or_else(|| iter.clone().count() - 1)
                })
            }
        }
    }

    pub fn divide<L, S>(&self, lines: L) -> RangeVec 
    where
        L: Iterator<Item = S> + Clone + Send + Sync,
        S: AsRef<str> + Send
    {
        let start_pattern = match self {
            Self::Enclosures { top, .. } => top,
            Self::Delimited { prefix, .. } => prefix,
            Self::Headings { fuzzed, strict, .. } => strict.as_ref().unwrap_or(fuzzed),
        };

        let chunk_filter = |(start, line): (usize, S)| start_pattern
            .is_match(line.as_ref())
            .then(|| InclRange::new(start, self.balancer(lines.clone())(&start)));

        lines.clone()
            .enumerate()
            .par_bridge()
            .filter_map(chunk_filter)
            .collect::<Vec<_>>()
            .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indoc::indoc;

    const DELIMITED_TEXT: &str = indoc!(
        "Foo {
            a: 0,
            b: Bar {
                c: 0,
                d: 1,
            }
        }
        Bar {
            c: 0,
            d: 1,
            e: 2,
        }
        Foo {
            a: 0,
            b: Bar {
                c: 0,
                d: 1,
            }
        }"
    );

    const HEADINGS_TEXT: &str = indoc!(
        "[config]
        a = 0
        b = 1
        
        [entities]
        Friendly, \"steve\", 35, 24, 10
        Enemy, \"bob\", 20, 12, 5"
    );

    const INDENTED_HEADINGS_TEXT: &str = indoc!(
        "[config]
        a = 0
        b = 1
        
        [entities]
            [Friendly]
                name = \"steve\"
                health = 35
                damage = 24
                speed = 10

            [Enemy]
                name = \"bob\"
                health = 20
                damage = 12
                speed = 5

        [enviorment]
            [tree]
                height = 10
                width = 20
                leaves = [
                    [0, 0],
                    [1, 1],
                    [2, 2],
                ]

            [grass]
                density = 0.5
                color = [0.5, 0.5, 0.5]"
    );

    //  I don't know why the hell anyone would use XAML for this kind of data but sure
    //  It can do it if someone has that use case.
    const ENCLOSED_TEXT: &str = indoc!(
        "<Foo>
            <val a=0/>
            <Bar>
                <val c=0/>
                <val d=1/>
            </Bar>
        </Foo>
        <Bar>
            <val c=0/>
            <val d=1/>
            <val e=2/>
        </Bar>
        <Foo bazz=true>
            <val a=0/>
            <Bar>
                <val c=0/>
                <val d=1/>
            </Bar>
        </Foo>"
    );

    #[test]
    fn delimited_divider() {
        let divider = Divider::Delimited {
            prefix: Regex::new(r"Foo").unwrap(),
            open: Regex::new(r"\{").unwrap(),
            close: Regex::new(r"\}").unwrap(),
        };

        let mut results = divider.divide(DELIMITED_TEXT.lines());
        results.sort_by_key(|range| range.upper);
        assert_eq!(
            results,
            RangeVec::from([(0, 6), (12, 18)])
        );
    }

    #[test]
    fn headings_divider() {
        let divider = Divider::Headings {
            fuzzed: Regex::new(r"\[.*\]").unwrap(),
            strict: Some(Regex::new(r"\[entities\]").unwrap()),
            indent: String::from(""),
        };

        let mut results = divider.divide(HEADINGS_TEXT.lines());
        results.sort_by_key(|range| range.upper);
        assert_eq!(results, RangeVec::from([(4, 6)]));
    }

    #[test]
    fn headings_divider_indented() {
        let divider = Divider::Headings {
            fuzzed: Regex::new(r"\[[a-z|A-Z]*\]").unwrap(),
            strict: Some(Regex::new(r"\[entities\]").unwrap()),
            indent: String::from("    "),
        };

        let mut results = divider.divide(INDENTED_HEADINGS_TEXT.lines());
        results.sort_by_key(|range| range.upper);
        assert_eq!(results, RangeVec::from([(4, 16)]));
    }

    #[test]
    fn enclosed_divider() {
        let divider = Divider::Enclosures {
            top: Regex::new(r"<Foo.*>").unwrap(),
            bottom: Regex::new(r"</Foo>").unwrap(),
        };

        let mut results = divider.divide(ENCLOSED_TEXT.lines());
        results.sort_by_key(|range| range.upper);
        assert_eq!(
            results,
            RangeVec::from([(0, 6), (12, 18)])
        );
    }
}
