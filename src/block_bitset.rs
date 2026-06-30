use crate::NodeSet;

/// Bits in one `usize` word: 64 on a 64-bit target, 32 on a 32-bit target.
///
/// The block words are `usize` rather than a fixed `u64` so the per-word bit
/// math runs at the target's native width; on a 32-bit target `usize` avoids
/// the emulated 64-bit operations a `u64` bitset would incur.
const BITS_PER_WORD: usize = core::mem::size_of::<usize>() * 8;

/// Two-level lazily-allocated dense bitset ("block bitmap" / uncompressed Roaring).
///
/// A directory of optional fixed-size dense blocks; a block is allocated only on
/// first write. Construction touches just the directory (one pointer per block),
/// so it is cheap for the many tiny BFSs; resident memory grows with the number
/// of distinct blocks touched; and for a whole-graph BFS it converges to a plain
/// bitset (~`num_nodes` bits) with **no promotion copy** (unlike
/// [`crate::AdaptiveNodeSet`]) and **no per-element container dispatch** (unlike
/// `RoaringTreemap`).
///
/// `WORDS` is the number of `usize` words per block, so a block covers
/// `BLOCK_BITS = WORDS * BITS_PER_WORD` node ids. The default (`WORDS = 1024`)
/// covers 65536 ids per block on a 64-bit target, matching Roaring's chunk size
/// so a touched block stays in L1/L2 and the top-level directory stays
/// cache-resident.
///
/// Derived from the Roaring bitmap split / cache-sizing rules (Chambi, Lemire,
/// Kaser, Godin, "Better bitmap performance with Roaring bitmaps", SPE 2016,
/// <https://arxiv.org/pdf/1402.6407>) restricted to bitmap containers with lazy
/// allocation.
pub struct BlockBitset<const WORDS: usize = 1024> {
    dir: Box<[Option<Box<[usize; WORDS]>>]>,
}

impl<const WORDS: usize> BlockBitset<WORDS> {
    /// Number of node ids covered by one block.
    const BLOCK_BITS: usize = WORDS * BITS_PER_WORD;

    /// Forces a compile-time error for the degenerate `WORDS == 0` block size,
    /// which would otherwise make `BLOCK_BITS == 0` and divide by zero in `new`.
    const ASSERT_WORDS_NONZERO: () = assert!(WORDS > 0, "BlockBitset requires WORDS > 0");
}

impl<const WORDS: usize> NodeSet for BlockBitset<WORDS> {
    fn new(num_nodes: usize) -> Self {
        const { Self::ASSERT_WORDS_NONZERO };
        let blocks = num_nodes.div_ceil(Self::BLOCK_BITS).max(1);
        Self {
            dir: (0..blocks).map(|_| None).collect(),
        }
    }

    #[inline(always)]
    fn insert(&mut self, node: usize) {
        let block = self.dir[node / Self::BLOCK_BITS].get_or_insert_with(|| Box::new([0usize; WORDS]));
        block[(node % Self::BLOCK_BITS) / BITS_PER_WORD] |= 1usize << (node % BITS_PER_WORD);
    }

    #[inline(always)]
    fn contains(&self, node: usize) -> bool {
        match &self.dir[node / Self::BLOCK_BITS] {
            Some(block) => block[(node % Self::BLOCK_BITS) / BITS_PER_WORD] & (1usize << (node % BITS_PER_WORD)) != 0,
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
        let mut set: BlockBitset = BlockBitset::new(1_000);
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
        let mut set: BlockBitset = BlockBitset::new(n);
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
        let mut set: BlockBitset = BlockBitset::new(128);
        set.insert(42);
        set.insert(42);
        assert!(set.contains(42));
        assert!(!set.contains(43));
    }

    #[test]
    fn custom_words_derives_block_size() {
        // WORDS = 2 => BLOCK_BITS = 2 * BITS_PER_WORD, so block boundaries fall
        // at multiples of 2*BITS_PER_WORD regardless of target word width.
        const W: usize = 2;
        let block_bits = W * BITS_PER_WORD;
        let mut set: BlockBitset<W> = BlockBitset::new(3 * block_bits);
        // One id in each of blocks 0, 1, 2, plus a second-word id within block 0.
        let present = [0usize, BITS_PER_WORD, block_bits, 2 * block_bits];
        for &v in &present {
            set.insert(v);
        }
        for &v in &present {
            assert!(set.contains(v), "missing {v}");
        }
        assert!(!set.contains(1));
        assert!(!set.contains(block_bits + 1));
    }
}
