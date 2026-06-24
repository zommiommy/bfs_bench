use crate::NodeSet;

/// Number of node ids covered by one block: 65536 ids => one 8 KiB block of
/// 1024 `u64` words. This matches Roaring's chunk size so a touched block stays
/// in L1/L2 and the top-level directory stays cache-resident.
const BLOCK_BITS: usize = 1 << 16;
const WORDS: usize = BLOCK_BITS / 64;

/// Two-level lazily-allocated dense bitset ("block bitmap" / uncompressed Roaring).
///
/// A directory of optional fixed-size dense blocks; a block is allocated only on
/// first write. Construction touches just the directory (one pointer per 65536
/// ids, ~4 MB at 34e9), so it is cheap for the many tiny BFSs; resident memory
/// grows with the number of distinct blocks touched; and for a whole-graph BFS it
/// converges to a plain bitset (~`num_nodes` bits) with **no promotion copy**
/// (unlike [`crate::AdaptiveNodeSet`]) and **no per-element container dispatch**
/// (unlike `RoaringTreemap`).
///
/// Derived from the Roaring bitmap split / cache-sizing rules (Chambi, Lemire,
/// Kaser, Godin, "Better bitmap performance with Roaring bitmaps", SPE 2016,
/// <https://arxiv.org/pdf/1402.6407>) restricted to bitmap containers with lazy
/// allocation.
pub struct BlockBitset {
    dir: Box<[Option<Box<[u64; WORDS]>>]>,
}

impl NodeSet for BlockBitset {
    fn new(num_nodes: usize) -> Self {
        let blocks = num_nodes.div_ceil(BLOCK_BITS).max(1);
        Self {
            dir: (0..blocks).map(|_| None).collect(),
        }
    }

    #[inline(always)]
    fn insert(&mut self, node: usize) {
        let block = self.dir[node / BLOCK_BITS].get_or_insert_with(|| Box::new([0u64; WORDS]));
        block[(node % BLOCK_BITS) / 64] |= 1u64 << (node % 64);
    }

    #[inline(always)]
    fn contains(&self, node: usize) -> bool {
        match &self.dir[node / BLOCK_BITS] {
            Some(block) => block[(node % BLOCK_BITS) / 64] & (1u64 << (node % 64)) != 0,
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn matches_reference_within_one_block() {
        let mut set = BlockBitset::new(1_000);
        let mut reference = HashSet::new();
        for &v in &[0usize, 1, 63, 64, 65, 999] {
            set.insert(v);
            reference.insert(v);
        }
        for v in 0usize..1_000 {
            assert_eq!(set.contains(v), reference.contains(&v), "mismatch at {v}");
        }
    }

    #[test]
    fn spans_blocks_and_unallocated_directory() {
        // 300k ids => 5 blocks of 65536 ids each (directory indices 0..=4).
        let n = 300_000usize;
        let mut set = BlockBitset::new(n);
        // Touch only blocks 0 and 2, leaving blocks 1, 3, 4 unallocated.
        let present = [0usize, 65_535, 131_072, 196_607];
        for &v in &present {
            set.insert(v);
        }
        for &v in &present {
            assert!(set.contains(v), "missing {v}");
        }
        // Some(block) present but the queried bit is clear.
        assert!(!set.contains(1));
        assert!(!set.contains(131_073));
        // None path: ids inside never-allocated directory entries (blocks 1, 3, 4).
        assert!(!set.contains(100_000));
        assert!(!set.contains(200_000));
        assert!(!set.contains(280_000));
    }

    #[test]
    fn idempotent_insert() {
        let mut set = BlockBitset::new(128);
        set.insert(42);
        set.insert(42);
        assert!(set.contains(42));
        assert!(!set.contains(43));
    }
}
