use std::collections::HashSet;
use std::hash::{BuildHasher, Hash};

use crate::{IterableNodeSet, NodeSet};

const SHIFT_BITS: u32 = 32;

/// A bucket able to store the low 32 bits of node ids for [`SparseRadixSet32`].
///
/// Implemented for any [`HashSet`] whose hasher is [`Default`], so the concrete
/// hashing algorithm can be chosen as a type parameter (e.g. `FxHashSet<u32>` or
/// `RapidHashSet<u32>`).
pub trait Bucket<T> {
    fn new() -> Self;
    fn len(&self) -> usize;
    fn insert(&mut self, val: T) -> bool;
    fn contains(&self, val: T) -> bool;
    fn iter(&self) -> impl Iterator<Item=T>;
}

impl<T: Hash + Eq + Copy, H: BuildHasher + Default> Bucket<T> for HashSet<T, H> {
    #[inline(always)]
    fn new() -> Self {
        Default::default()
    }
    #[inline(always)]
    fn len(&self) -> usize {
        HashSet::len(self)
    }
    #[inline(always)]
    fn insert(&mut self, val: T) -> bool {
        HashSet::insert(self, val)
    }
    #[inline(always)]
    fn contains(&self, val: T) -> bool {
        HashSet::contains(self, &val)
    }
    #[inline(always)]
    fn iter(&self) -> impl Iterator<Item=T> {
        HashSet::iter(self).copied()
    }
}

/// Set of node ids that splits every id into a 32-bit high part (used as a bucket
/// index) and a 32-bit low part (stored inside the bucket).
///
/// Proposed by vlorentz in swh-graph#4808 (note 258217): node ids fit on 37 bits,
/// so storing only the low 32 bits in the hash set (instead of the full `usize`)
/// halves per-element memory and is friendlier to the CPU cache. The high bits
/// select one of `2^5` buckets.
pub struct SparseRadixSet32<B: Bucket<u32>> {
    buckets: Vec<B>,
}

#[cold]
#[inline(never)]
fn cold() {}

impl<B: Bucket<u32>> SparseRadixSet32<B> {
    pub fn new(num_values: usize) -> Self {
        let (high, _low) = Self::split_value(num_values);
        Self {
            buckets: (0..=high).map(|_| B::new()).collect(),
        }
    }

    #[inline(always)]
    /// returns (high, low)
    fn split_value(val: usize) -> (u32, u32) {
        // usize fits in u64 on all supported targets, so this is infallible.
        let val = u64::try_from(val).unwrap();
        let high = val >> SHIFT_BITS;
        let low = val & ((1u64 << SHIFT_BITS) - 1);

        // `high` is `val >> 32` and `low` is masked to 32 bits, so both fit in u32.
        (u32::try_from(high).unwrap(), u32::try_from(low).unwrap())
    }

    pub fn insert(&mut self, val: usize) -> bool {
        let (high, low) = Self::split_value(val);
        // u32 always fits in usize on the >=32-bit targets this runs on.
        let high = usize::try_from(high).unwrap();
        if high >= self.buckets.len() {
            cold();
            panic!(
                "Attempted to insert {val}, but max value is {}",
                (1 << self.buckets.len()) - 1
            );
        }
        // SAFETY: `high < self.buckets.len()` is checked on the line above, so
        // `high` is an in-bounds index; the derived pointer is aligned and
        // dereferenceable for one `B`, and `&mut self` makes the returned
        // `&mut B` the only live reference into `buckets`.
        unsafe { self.buckets.get_unchecked_mut(high) }.insert(low)
    }

    pub fn contains(&self, val: usize) -> bool {
        let (high, low) = Self::split_value(val);
        // u32 always fits in usize on the >=32-bit targets this runs on.
        let high = usize::try_from(high).unwrap();
        if high >= self.buckets.len() {
            cold();
            return false;
        }
        // SAFETY: `high < self.buckets.len()` is checked on the line above, so
        // `high` is an in-bounds index; the derived pointer is aligned and
        // dereferenceable for one `B`, and `&self` yields a shared `&B` that
        // cannot alias any active `&mut B`.
        unsafe { self.buckets.get_unchecked(high) }.contains(low)
    }
}

impl<B: Bucket<u32>> NodeSet for SparseRadixSet32<B> {
    #[inline(always)]
    fn new(num_nodes: usize) -> Self {
        SparseRadixSet32::new(num_nodes)
    }
    #[inline(always)]
    fn len(&self) -> usize {
        self.buckets.iter().map(|bucket| bucket.len()).sum()
    }
    #[inline(always)]
    fn insert(&mut self, node: usize) {
        SparseRadixSet32::insert(self, node);
    }
    #[inline(always)]
    fn contains(&self, node: usize) -> bool {
        SparseRadixSet32::contains(self, node)
    }
}

impl<B: Bucket<u32>> IterableNodeSet for SparseRadixSet32<B> {
    fn iter(&self) -> impl Iterator<Item = usize> {
        self.buckets.iter().enumerate().flat_map(|(high, bucket)| {
            let high = u64::try_from(high).unwrap();
            bucket.iter().map(move |low| {
                let v = (high << SHIFT_BITS) | u64::from(low);
                usize::try_from(v).unwrap()
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type FxBucket = fxhash::FxHashSet<u32>;

    #[test]
    fn matches_reference_within_single_bucket() {
        let mut set = SparseRadixSet32::<FxBucket>::new(1_000);
        let mut reference = HashSet::new();
        for &v in &[0usize, 1, 7, 42, 999] {
            assert_eq!(set.insert(v), reference.insert(v));
        }
        for v in 0usize..1_000 {
            assert_eq!(set.contains(v), reference.contains(&v), "mismatch at {v}");
        }
        // Re-inserting an existing value reports "already present".
        assert!(!set.insert(42));
    }

    #[test]
    fn spans_multiple_buckets_above_u32() {
        // num_values just past 2^32 forces a second bucket (high == 1).
        let max = (1usize << 32) + 10;
        let mut set = SparseRadixSet32::<FxBucket>::new(max);

        let base = 5usize; // bucket 0, low bits = 5
        let crossed = (1usize << 32) + 5; // bucket 1, identical low bits

        set.insert(base);
        assert!(set.contains(base));
        // Same low 32 bits but different high bits must not alias.
        assert!(!set.contains(crossed));

        set.insert(crossed);
        assert!(set.contains(crossed));
        assert!(set.contains(base));
        // A different low value in the high bucket is absent.
        assert!(!set.contains((1usize << 32) + 6));
    }

    #[test]
    fn radix_boundary_values() {
        // Exercise the exact split boundary: u32::MAX and u32::MAX + 1.
        let u32_max = usize::try_from(u32::MAX).unwrap(); // 0xFFFF_FFFF, bucket 0
        let just_over = u32_max + 1; // 0x1_0000_0000, bucket 1, low bits = 0
        let mut set = SparseRadixSet32::<FxBucket>::new(just_over + 1);
        set.insert(0);
        set.insert(u32_max);
        set.insert(just_over);
        assert!(set.contains(0));
        assert!(set.contains(u32_max));
        assert!(set.contains(just_over));
        // Low bits 1 in bucket 0 was never inserted.
        assert!(!set.contains(1));
    }

    #[test]
    fn contains_out_of_range_high_returns_false() {
        // Only one bucket (high == 0); querying a high bucket must not panic.
        let set = SparseRadixSet32::<FxBucket>::new(10);
        assert!(!set.contains((1usize << 32) + 3));
    }
}
