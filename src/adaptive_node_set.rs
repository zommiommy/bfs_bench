use crate::{IterableNodeSet, NodeSet};
use std::collections::HashSet;
use sux::prelude::*;

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
pub enum AdaptiveNodeSet<SPARSE: IterableNodeSet = HashSet<usize>, DENSE: NodeSet = BitVec> {
    Sparse { max_items: usize, data: SPARSE },
    Dense { data: DENSE },
}

impl<SPARSE: IterableNodeSet, DENSE: NodeSet> NodeSet for AdaptiveNodeSet<SPARSE, DENSE> {
    /// Creates an empty `AdaptiveNodeSet` that may only store node ids from `0` to `max_items-1`
    fn new(max_items: usize) -> Self {
        AdaptiveNodeSet::Sparse {
            max_items,
            data: SPARSE::new(max_items),
        }
    }

    #[inline(always)]
    fn len(&self) -> usize {
        match self {
            AdaptiveNodeSet::Sparse { max_items: _, data } => data.len(),
            AdaptiveNodeSet::Dense { data } => data.len(),
        }
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
                    let mut new_data = DENSE::new(*max_items);
                    for node in data.iter() {
                        new_data.insert(node);
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
            AdaptiveNodeSet::Sparse { max_items: _, data } => data.contains(node),
            AdaptiveNodeSet::Dense { data } => data.contains(node),
        }
    }
}
