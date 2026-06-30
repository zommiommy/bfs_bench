
use std::collections::HashSet;
use sux::prelude::*;
use crate::NodeSet;

/// Constant controlling when a [`AdaptiveNodeSet`] should be promoted from sparse to dense.
///
/// Promotion happens when the number of items in a [`AdaptiveNodeSet`] times this constant
/// is greater than the maximum value.
///
/// The value was computed experimentally, using "find-earliest-revision" (which runs
/// many BFSs of highly heterogeneous sizes) on the 2023-09-06 graph, running on
/// Software Heritage's Maxxi computer (Xeon Gold 6342 CPU @ 2.80GHz, 96 threads, 4TB RAM)
/// That graph contains 34 billion nodes, and performance increases continuously when
/// increasing up to 100 millions, then plateaus up to 1 billion; though the latter
/// uses a little less memory most of the time.
const PROMOTION_THRESHOLD: usize = 64;

/// Implementation of [`NodeSet`] that dynamically changes the underlying representation
/// based on its content
///
/// The current implementation is initialized with a [`HashSet`], but switches to
/// [`BitVec`] once the data becomes dense.
///
/// This has the advantage of allocating little memory if there won't be many elements,
/// but avoiding the overhead of [`HashSet`] when there are.
///
/// The representation switches from sparse to dense as items accumulate:
///
/// ```text
/// let mut node_set = AdaptiveNodeSet::new(100);
/// // Sparse { max_items: 100, data: {} }
/// node_set.insert(10);
/// // Sparse { max_items: 100, data: {10} }
/// for i in 20..30 { node_set.insert(i); }
/// // Dense { data: BitVec { bits: [1072694272, 0], len: 100 } }
/// ```
#[derive(Debug)]
pub enum AdaptiveNodeSet {
    Sparse {
        max_items: usize,
        data: HashSet<usize>,
    },
    Dense {
        data: BitVec,
    },
}

impl AdaptiveNodeSet {
    /// Creates an empty `AdaptiveNodeSet` that may only store node ids from `0` to `max_items-1`
    #[inline(always)]
    pub fn new(max_items: usize) -> Self {
        AdaptiveNodeSet::Sparse {
            max_items,
            data: HashSet::new(),
        }
    }

    /// Like [`NodeSet::insert`], but returns whether `node` was newly inserted
    /// (`true`) or already present (`false`), matching [`HashSet::insert`].
    ///
    /// Used by the `AdaptiveBucket` wrapper so [`crate::sparse_radix_set::SparseRadixSet32`]
    /// can honour its `Bucket::insert` contract without an extra membership probe.
    /// The promotion logic is kept identical to [`NodeSet::insert`].
    #[inline(always)]
    pub fn insert_new(&mut self, node: usize) -> bool {
        match self {
            AdaptiveNodeSet::Sparse { max_items, data } => {
                let inserted = data.insert(node);
                if data.len() > *max_items / PROMOTION_THRESHOLD {
                    // Promote the hashset to a bitvec
                    let mut new_data = BitVec::new(*max_items);
                    for node in data.iter() {
                        new_data.insert(*node);
                    }
                    *self = AdaptiveNodeSet::Dense { data: new_data };
                }
                inserted
            }
            AdaptiveNodeSet::Dense { data } => {
                let present = data.contains(node);
                data.insert(node);
                !present
            }
        }
    }
}

impl NodeSet for AdaptiveNodeSet {
    fn new(num_nodes: usize) -> Self {
        AdaptiveNodeSet::new(num_nodes)
    }

    /// Adds a node to the set
    ///
    /// # Panics
    ///
    /// If `node` is larger or equal to the `max_items` value passed to [`AdaptiveNodeSet::new`].
    #[inline(always)]
    fn insert(&mut self, node: usize) {
        match self {
            AdaptiveNodeSet::Sparse { max_items, data } => {
                data.insert(node);
                if data.len() > *max_items / PROMOTION_THRESHOLD {
                    // Promote the hashset to a bitvec
                    let mut new_data = BitVec::new(*max_items);
                    for node in data.iter() {
                        new_data.insert(*node);
                    }
                    *self = AdaptiveNodeSet::Dense { data: new_data };
                }
            }
            AdaptiveNodeSet::Dense { data } => data.insert(node),
        }
    }

    /// Returns whether the node is part of the set
    ///
    /// # Panics
    ///
    /// If `node` is larger or equal to the `max_items` value passed to [`AdaptiveNodeSet::new`].
    #[inline(always)]
    fn contains(&self, node: usize) -> bool {
        match self {
            AdaptiveNodeSet::Sparse { max_items: _, data } => data.contains(&node),
            AdaptiveNodeSet::Dense { data } => data.contains(node),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_new_reports_membership_across_promotion() {
        // max_items / PROMOTION_THRESHOLD == 128 / 64 == 2, so the set promotes
        // to Dense once it holds more than 2 elements (on the 3rd distinct one).
        let mut set = AdaptiveNodeSet::new(128);
        assert!(matches!(set, AdaptiveNodeSet::Sparse { .. }));

        // Sparse path: new vs. duplicate.
        assert!(set.insert_new(10));
        assert!(!set.insert_new(10));
        assert!(set.insert_new(20));
        assert!(matches!(set, AdaptiveNodeSet::Sparse { .. }));

        // 3rd distinct element triggers promotion to Dense.
        assert!(set.insert_new(30));
        assert!(matches!(set, AdaptiveNodeSet::Dense { .. }));

        // Dense path: duplicate then new.
        assert!(!set.insert_new(30));
        assert!(set.insert_new(40));

        for &v in &[10usize, 20, 30, 40] {
            assert!(set.contains(v), "missing {v}");
        }
        for v in (0usize..128).filter(|v| ![10, 20, 30, 40].contains(v)) {
            assert!(!set.contains(v), "unexpected {v}");
        }
    }

    #[test]
    fn insert_new_matches_reference() {
        let mut set = AdaptiveNodeSet::new(1_000);
        let mut reference = HashSet::new();
        for &v in &[0usize, 1, 7, 7, 42, 42, 500, 999] {
            assert_eq!(set.insert_new(v), reference.insert(v), "mismatch inserting {v}");
        }
        for v in 0usize..1_000 {
            assert_eq!(set.contains(v), reference.contains(&v), "mismatch at {v}");
        }
    }
}
