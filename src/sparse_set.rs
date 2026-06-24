use crate::NodeSet;

/// Briggs-Torczon sparse set: a dense array of inserted keys plus a sparse index
/// validated by a cross-check, giving O(1) `insert`/`contains` with no re-zeroing.
///
/// `dense[..len]` holds inserted keys in insertion order; `sparse[k]` holds the
/// position of `k` in `dense`. Membership is the cross-check
/// `sparse[k] < len && dense[sparse[k]] == k`, so `sparse` never needs
/// initialization beyond the (lazily zeroed) allocation.
///
/// Preston Briggs & Linda Torczon, "An Efficient Representation for Sparse Sets",
/// ACM LOPLAS 2(1-4):59-69, 1993, <https://dl.acm.org/doi/10.1145/176454.176484>
/// (engineering writeup: <https://research.swtch.com/sparse>).
///
/// Memory is universe-sized: `sparse` is `num_nodes` words and `dense` grows to
/// at most `num_nodes` words, so this is only economical when the universe itself
/// is small/medium (the smaller graphs); at billions of nodes it needs hundreds
/// of GiB. Its signature wins (O(1) clear, ordered iteration) are unused by this
/// benchmark, so it serves as a cache-friendly reference point.
pub struct SparseSet {
    dense: Vec<usize>,
    sparse: Box<[usize]>,
    len: usize,
}

impl NodeSet for SparseSet {
    fn new(num_nodes: usize) -> Self {
        Self {
            dense: Vec::new(),
            sparse: vec![0usize; num_nodes].into_boxed_slice(),
            len: 0,
        }
    }

    #[inline(always)]
    fn insert(&mut self, node: usize) {
        if !self.contains(node) {
            self.sparse[node] = self.len;
            self.dense.push(node);
            self.len += 1;
        }
    }

    #[inline(always)]
    fn contains(&self, node: usize) -> bool {
        let i = self.sparse[node];
        i < self.len && self.dense[i] == node
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn matches_reference() {
        let mut set = SparseSet::new(1_000);
        let mut reference = HashSet::new();
        for &v in &[0usize, 1, 7, 42, 500, 999] {
            set.insert(v);
            reference.insert(v);
        }
        for v in 0usize..1_000 {
            assert_eq!(set.contains(v), reference.contains(&v), "mismatch at {v}");
        }
    }

    #[test]
    fn cross_check_rejects_stale_sparse_entries() {
        // sparse is zero-initialised, so key 0 must not appear present until inserted.
        let mut set = SparseSet::new(16);
        assert!(!set.contains(0));
        set.insert(5);
        assert!(!set.contains(0)); // sparse[0]==0 but dense[0]==5 != 0
        set.insert(0);
        assert!(set.contains(0));
        assert!(set.contains(5));
    }

    #[test]
    fn idempotent_insert() {
        let mut set = SparseSet::new(16);
        set.insert(3);
        set.insert(3);
        assert!(set.contains(3));
        assert_eq!(set.dense.len(), 1);
    }
}
