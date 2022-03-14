use std::collections::HashSet;
use crate::utils::*;
use std::fmt::Debug;
use serde::{Serialize, Deserialize};
use slice_diff_patch::*;
use diff::Result as DiffResult;
use itertools::Itertools;
use itertools::FoldWhile::{Continue, Done};

trait ChangeExt {
    fn offset(&self) -> usize;
    fn with_offest_as(self, offset: isize) -> Self;
}

impl<T> ChangeExt for Change<T> 
where
    T: Debug + PartialEq + Clone
{
    fn offset(&self) -> usize {
        match self {
            Change::Remove(i) => *i,
            Change::Insert((i, _)) => *i,
            Change::Update((i, _)) => *i,
        }
    }

    fn with_offest_as(self, offset: isize) -> Self {
        match self {
            Change::Remove(_) => Change::Remove(offset as usize),
            Change::Insert((_, s)) => Change::Insert((offset as usize, s)),
            Change::Update((_, s)) => Change::Update((offset as usize, s)),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct StructDiff {
    pub comment: String,
    pub changes: Vec<Change<String>>,
    pub removed: Vec<usize>,
    pub added: Vec<usize>
}

impl StructDiff {
    pub fn build_from(old: &str, new: &str, comment: &str) -> Self {
        let diff = diff::lines(old, new);

        let removed = diff
            .iter()
            .filter(|d| !matches!(d, DiffResult::Right(_)))
            .enumerate()
            .filter(|(_, line)| matches!(line, DiffResult::Left(_)))
            .map(|(i, _)| i);

        let added = diff
            .iter()
            .filter(|d| !matches!(d, DiffResult::Left(_)))
            .enumerate()
            .filter(|(_, line)| matches!(line, DiffResult::Right(_)))
            .map(|(i, _)| i);

        let changes = diff_diff(
            old.lines().map(|l| l.to_string()).collect::<Vec<_>>().as_ref(),
            new.lines().map(|l| l.to_string()).collect::<Vec<_>>().as_ref()
        );

        Self {
            comment: comment.to_string(),
            removed: removed.collect(),
            added: added.collect(),
            changes
        }
    }

    pub fn patch<L, S>(&self, original: L) -> Vec<String>
    where
        L: Iterator<Item = S> + Clone + Send + Sync,
        S: AsRef<str> + Send
    {
        let original = original
            .map(|l| l.as_ref().to_string())
            .collect::<Vec<_>>();
        
        patch(original.as_ref(), &self.changes)
    }

    pub fn extend(&mut self, other: Self) { // this algorithm can be improved
        {
            let removed = self
                .changes
                .iter()
                .filter(|c| matches!(c, Change::Remove(_)))
                .map(|c| c.offset())
                .collect::<HashSet<_>>();

            let change_shifts = self
                .changes
                .iter()
                .map(|change| match change {
                    Change::Remove(i) => (*i, -1),
                    Change::Insert((i, _)) => (*i, 1),
                    Change::Update((i, _)) => (*i, 0)
                })
                .collect::<HashSet<_>>();

            let de_duped = other.changes.iter().filter(
                |change| match change {
                    Change::Remove(i) => !removed.contains(i),
                    _ => true
                }
            );

            let new_changes = de_duped.cloned().map(|change| {
                let shifted = change_shifts.iter().fold_while(change.offset() as isize,
                    |acc, (offset, shift)| {
                        if (*offset as isize) < acc {
                            Continue(acc + shift)
                        } else {
                            Done(acc)
                        }
                    }
                );

                change.with_offest_as(shifted.into_inner())
            });

            self.changes.extend_from_slice(
                new_changes
                    .collect::<Vec<_>>()
                    .as_slice()
            );
        }

        {
            let removed = self
                .removed
                .iter()
                .collect::<HashSet<_>>();

            let mut shifts = other
                .removed
                .iter()
                .filter(|i| !removed.contains(i))
                .map(|i| (*i, -1))
                .chain(self.added.iter_mut().map(|i| (*i, 1)))
                .collect::<Vec<_>>();

            shifts.sort_by_key(|(i, _)| *i);

            let new_removed = other.removed.iter().map(|removal| {
                let shifted = shifts.iter().fold_while(*removal as isize,
                    |acc, (_, shift)| {
                        if (acc as isize) < acc {
                            Continue(acc + shift)
                        } else {
                            Done(acc)
                        }
                    }
                );

                shifted.into_inner() as usize
            });

            let new_added = other.added.iter().map(|addition| {
                let shift = shifts.iter().fold_while(*addition as isize,
                    |acc, (_, shift)| {
                        if (acc as isize) < acc {
                            Continue(acc + shift)
                        } else {
                            Done(acc)
                        }
                    }
                );

                shift.into_inner() as usize
            });

            self.comment = "Super Mod".to_string();
            self.removed.extend(new_removed.collect::<Vec<_>>().as_slice());
            self.added.extend(new_added.collect::<Vec<_>>().as_slice());
            self.removed.sort_unstable();
            self.added.sort_unstable();
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use indoc::indoc;

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

    const MODDED: &str = indoc!(
        "config: {
            scroll_speed: 0.1,
            view_distance: 10,
        },
        objs: {
            press: {
                lane: 2,
                time: 0,
                color: 0xFF0000,
            },
            hold: {
                lane: 2,
                time: 1,
                color: 0x00FF00,
            },
            hold: {
                lane: 2,
                time: 0,
                color: 0x0000FF,
            },
        }"
    );

    #[test]
    fn added_and_removed() {
        let modded = StructDiff::build_from(ORIGINAL, MODDED, "");
        assert_eq!(modded.removed, vec![6, 11, 12, 15]);
        assert_eq!(modded.added, vec![6, 11, 12, 15]);
    }

    #[test]
    fn patch() {
        let remade = StructDiff::build_from(ORIGINAL, MODDED, "").patch(ORIGINAL.lines());

        let expected_remade = MODDED
            .lines()
            .map(|s| s.to_string())
            .collect::<Vec<_>>();

        assert_eq!(remade, expected_remade);
    }

    const ALPHA_ORIGINAL: &str = indoc!(
        "a
        b
        c
        d
        e
        f"
    );

    const ALPHA_MODDED_1: &str = indoc!(
        "a
        c
        d
        e
        f"
    );

    const ALPHA_MODDED_2: &str = indoc!(
        "a
        b
        c
        d
        G
        e
        f"
    );

    const EXPECTED_ALPHA_1: &str = indoc!(
        "a
        c
        d
        G
        e
        f"
    );

    #[test]
    fn merge_mods() {
        let mut modded_1 = StructDiff::build_from(ALPHA_ORIGINAL, ALPHA_MODDED_1, "");
        let modded_2 = StructDiff::build_from(ALPHA_ORIGINAL, ALPHA_MODDED_2, "");

        modded_1.extend(modded_2);
        let patched = modded_1.patch(ALPHA_ORIGINAL.lines()).join("\n");
        assert_eq!(patched, EXPECTED_ALPHA_1);
    }
}
