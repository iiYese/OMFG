use derive_more::{Deref, DerefMut};
use itertools::Itertools;
use rayon::prelude::*;
//  this stupid thing has it's end private
//  which has to do with ranges being Iter
use std::ops::RangeInclusive;

//  which is why this is necessary
#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub struct InclRange {
    pub lower: usize,
    pub upper: usize,
}

impl InclRange {
    pub fn new(lower: usize, upper: usize) -> Self {
        Self { lower, upper }
    }

    pub fn iter(self) -> RangeInclusive<usize> {
        self.lower..=self.upper
    }

    pub fn contains(&self, &value: &usize) -> bool {
        self.lower <= value && value <= self.upper
    }
}

impl From<(usize, usize)> for InclRange {
    fn from((lower, upper): (usize, usize)) -> Self {
        Self::new(lower, upper)
    }
}

impl From<usize> for InclRange {
    fn from(value: usize) -> Self {
        Self::new(value, value)
    }
}

fn union(first: &InclRange, second: &InclRange) -> Option<InclRange> {
    let exists = first.contains(&second.lower) || second.contains(&first.lower);
    exists.then(|| InclRange {
        lower: first.lower.min(second.lower),
        upper: first.upper.max(second.upper),
    })
}

fn intersection(first: &InclRange, second: &InclRange) -> Option<InclRange> {
    let exists = first.contains(&second.lower) || second.contains(&first.lower);
    exists.then(|| InclRange {
        lower: first.lower.max(second.lower),
        upper: first.upper.min(second.upper),
    })
}

#[derive(Deref, DerefMut, PartialEq, Eq, Clone, Debug)]
pub struct RangeVec(Vec<InclRange>);

impl RangeVec {
    // consuming deref
    pub fn dewrap(self) -> Vec<InclRange> {
        self.0
    }

    pub fn pre_ops(mut self) -> Self {
        self.dedup();
        self.sort_by_key(|r| r.lower);
        self
    }

    fn flattened(self) -> Self {
        self.iter()
            .cloned()
            .coalesce(|prev, curr| union(&prev, &curr).ok_or((prev, curr)))
            .collect::<Vec<_>>()
            .into()
    }

    fn joined(self) -> Self {
        let join_adjacents = |prev: InclRange, curr: InclRange| {
            (curr.lower as isize - prev.upper as isize <= 1)
                .then(|| InclRange::new(prev.lower, curr.upper))
                .ok_or((prev, curr))
        };

        self.iter()
            .cloned()
            .coalesce(join_adjacents)
            .collect::<Vec<_>>()
            .into()
    }

    pub fn union_with(self, other: Self) -> Self {
        let ret: Self = [self.pre_ops().dewrap(), other.pre_ops().dewrap()]
            .into_iter()
            .kmerge_by(|a, b| a.lower < b.lower)
            .collect::<Vec<_>>()
            .into();

        ret.flattened()
    }

    pub fn intersection_with(self, other: Self) -> Self {
        self.pre_ops()
            .iter()
            .cartesian_product(other.pre_ops().iter())
            .map(|(a, b)| intersection(a, b))
            .flatten()
            .collect::<Vec<_>>()
            .into()
    }

    pub fn that_overlap(self, other: Self) -> Self {
        let other = other.pre_ops();
        self.pre_ops()
            .iter()
            .filter(|a| other.par_iter().any(|b| intersection(a, b).is_some()))
            .cloned()
            .collect::<Vec<_>>()
            .into()
    }

    pub fn inverse(self, exclusive_lim: usize) -> Self {
        let mut offsets = self
            .pre_ops()
            .flattened()
            .joined()
            .dewrap()
            .into_iter()
            .filter(|r| r.lower != r.upper)
            .flat_map(|r| [r.lower, r.upper]);

        let head = offsets
            .next()
            .and_then(|start| (0 < start).then(|| vec![InclRange::new(0, start)].into_iter()))
            .unwrap_or_else(|| vec![].into_iter());

        let body = offsets
            .chain([exclusive_lim - 1].into_iter())
            .tuple_windows()
            .step_by(2)
            .map(|(lower, upper)| InclRange::new(lower, upper));

        head.chain(body)
            .collect::<Vec<_>>()
            .into()
    }
}

impl<T, U> From<T> for RangeVec
where
    T: IntoIterator<Item = U>,
    InclRange: From<U>,
{
    fn from(iter: T) -> Self {
        Self(iter.into_iter().map(InclRange::from).collect::<Vec<_>>())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    type Range = std::ops::Range<usize>;

    #[test]
    fn range_vec_flatten() {
        let rv = RangeVec::from([(0, 10), (5, 9), (20, 30), (21, 25)]);

        assert_eq!(rv.flattened(), [(0, 10), (20, 30)].into());
    }

    #[test]
    fn range_vec_union() {
        let rv = RangeVec::from([(0, 10)]);
        let other = RangeVec::from([(6, 20)]);
        let expected = RangeVec::from([(0, 20)]);
        assert_eq!(rv.union_with(other), expected);

        let rv = RangeVec::from([(0, 10)]);
        let other = RangeVec::from([(10, 20)]);
        let expected = RangeVec::from([(0, 20)]);
        assert_eq!(rv.union_with(other), expected);
    }

    #[test]
    fn range_vec_intersection() {
        let rv = RangeVec::from([(0, 10)]);
        let other = RangeVec::from([(6, 20)]);
        let expected = RangeVec::from([(6, 10)]);
        assert_eq!(rv.intersection_with(other), expected);

        let rv = RangeVec::from([(0, 10)]);
        let other = RangeVec::from([(10, 20)]);
        let expected = RangeVec::from([(10, 10)]);
        assert_eq!(rv.intersection_with(other), expected);

        let rv = RangeVec::from([(0, 10)]);
        let other = RangeVec::from([(2, 8)]);
        let expected = RangeVec::from([(2, 8)]);
        assert_eq!(rv.intersection_with(other), expected);

        let rv = RangeVec::from([(0, 10)]);
        let other = RangeVec::from([(0, 10)]);
        let expected = RangeVec::from([(0, 10)]);
        assert_eq!(rv.intersection_with(other), expected);
    }

    #[test]
    fn range_vec_joined() {
        let rv = RangeVec::from([(0, 0), (2, 3), (3, 5), (7, 8)]);
        let expected = RangeVec::from([(0, 0), (2, 5), (7, 8)]);
        assert_eq!(rv.joined(), expected);

        let rv = RangeVec::from([(0, 0), (1, 1), (3, 5), (7, 8)]);
        let expected = RangeVec::from([(0, 1), (3, 5), (7, 8)]);
        assert_eq!(rv.joined(), expected);

        let rv = RangeVec::from([(0, 0), (2, 5), (4, 6), (8, 9), (11, 11)]);
        let expected = RangeVec::from([(0, 0), (2, 6), (8, 9), (11, 11)]);
        assert_eq!(rv.joined(), expected);
    }

    #[test]
    fn range_vec_inverse() {
        let rv = RangeVec::from([(0, 2), (4, 7), (12, 20)]);
        let expected = RangeVec::from([(2, 4), (7, 12), (20, 24)]);
        assert_eq!(rv.inverse(25), expected);
        
        let rv = RangeVec::from([(0, 2), (4, 7), (12, 20)]);
        let expected = RangeVec::from([(2, 4), (7, 12), (20, 24)]);
        assert_eq!(rv.inverse(25), expected);

        let rv = RangeVec::from([(0, 2), (4, 7), (12, 20), (23, 25)]);
        let expected = RangeVec::from([(2, 4), (7, 12), (20, 23), (25, 25)]);
        assert_eq!(rv.inverse(26), expected);
    }

    #[test]
    fn range_vec_that_overlap() {
        let rv = RangeVec::from([(0, 10), (12, 20), (22, 30)]);
        let other = RangeVec::from([(2, 4), (7, 12)]);
        let expected = RangeVec::from([(0, 10), (12, 20)]);
        assert_eq!(rv.that_overlap(other), expected);
    }
}
