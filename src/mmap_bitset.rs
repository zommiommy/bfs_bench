use crate::NodeSet;

/// Bits in one `usize` word: 64 on a 64-bit target, 32 on a 32-bit target.
const BITS_PER_WORD: usize = core::mem::size_of::<usize>() * 8;

/// Flat dense bitset whose backing store is a single anonymous `mmap`, left to
/// the kernel's demand paging instead of an explicit block directory.
///
/// This is the "hardware-paginated" counterpart to [`crate::BlockBitset`]: one
/// contiguous `num_nodes`-bit mapping is reserved up front, but
/// `MAP_PRIVATE | MAP_ANONYMOUS` makes every page demand-zero, so physical RAM
/// is committed by the kernel one page (typically 4 KiB) at a time on first
/// touch. Reserving the universe costs only virtual address space (e.g. ~4.25
/// GiB for 34e9 nodes), and resident memory grows with the pages actually
/// written.
///
/// Versus [`crate::BlockBitset`], the directory is gone — `contains`/`insert`
/// are plain word ops with no `Option` branch — but the BFS loop probes
/// `contains` before `insert`, so the first read of an untouched page takes a
/// minor page fault (kernel trap mapping the shared zero page) where
/// `BlockBitset` would answer `false` from a `None` slot without leaving
/// userspace. This type measures whether the kernel's pagination beats that
/// userspace directory on a given graph/traversal shape.
///
/// Reset is `munmap` on drop (the benchmark builds a fresh set per BFS), which
/// returns every committed frame and tears down the page-table entries in one
/// syscall.
pub struct MmapBitset {
    /// Pointer to the start of the mapping (`words * size_of::<usize>()` bytes).
    ptr: *mut usize,
    /// Number of `usize` words in the mapping.
    words: usize,
}

impl MmapBitset {
    /// Byte length of the mapping.
    #[inline(always)]
    fn len_bytes(&self) -> usize {
        self.words * core::mem::size_of::<usize>()
    }
}

impl NodeSet for MmapBitset {
    fn new(num_nodes: usize) -> Self {
        // `mmap` rejects a length of 0, so always map at least one word.
        let words = num_nodes.div_ceil(BITS_PER_WORD).max(1);
        let len_bytes = words * core::mem::size_of::<usize>();

        // SAFETY: a standard anonymous private mapping request: null `addr`
        // lets the kernel choose the address, `len_bytes > 0` (ensured above),
        // `fd == -1` with `MAP_ANONYMOUS`, and `offset == 0`. The pages are
        // demand-zero, so the mapping starts logically all-false.
        let ptr = unsafe {
            libc::mmap(
                core::ptr::null_mut(),
                len_bytes,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
                -1,
                0,
            )
        };
        assert!(ptr != libc::MAP_FAILED, "mmap of {len_bytes} bytes failed");

        Self {
            ptr: ptr.cast::<usize>(),
            words,
        }
    }

    #[inline(always)]
    fn insert(&mut self, node: usize) {
        let word = node / BITS_PER_WORD;
        let bit = node % BITS_PER_WORD;
        debug_assert!(word < self.words, "node {node} out of range");
        // SAFETY: `node < num_nodes` (BFS only inserts valid node ids), and
        // `words == ceil(num_nodes / BITS_PER_WORD)`, so `word < self.words`
        // and the offset is in bounds; `&mut self` makes this the only access.
        unsafe {
            let p = self.ptr.add(word);
            *p |= 1usize << bit;
        }
    }

    #[inline(always)]
    fn contains(&self, node: usize) -> bool {
        let word = node / BITS_PER_WORD;
        let bit = node % BITS_PER_WORD;
        debug_assert!(word < self.words, "node {node} out of range");
        // SAFETY: as in `insert`, `word < self.words`, so the read is in
        // bounds; `&self` yields a shared read that cannot alias a live `&mut`.
        unsafe {
            let p = self.ptr.add(word);
            *p & (1usize << bit) != 0
        }
    }
}

impl Drop for MmapBitset {
    fn drop(&mut self) {
        // SAFETY: `ptr`/`len_bytes` are exactly the mapping returned by `mmap`
        // in `new` (never freed elsewhere), so this unmaps the whole region.
        unsafe {
            libc::munmap(self.ptr.cast::<libc::c_void>(), self.len_bytes());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn matches_reference() {
        let mut set = MmapBitset::new(1_000);
        let mut reference = HashSet::new();
        for &v in &[0usize, 1, 63, 64, 65, 127, 500, 999] {
            set.insert(v);
            reference.insert(v);
        }
        for v in 0usize..1_000 {
            assert_eq!(set.contains(v), reference.contains(&v), "mismatch at {v}");
        }
    }

    #[test]
    fn starts_empty() {
        // Demand-zero pages mean every bit reads false before any insert.
        let set = MmapBitset::new(10_000);
        for v in 0usize..10_000 {
            assert!(!set.contains(v), "unexpected member {v}");
        }
    }

    #[test]
    fn word_boundary_bits_are_independent() {
        let mut set = MmapBitset::new(256);
        for &v in &[63usize, 64, 191, 192] {
            set.insert(v);
        }
        for v in 0usize..256 {
            let expected = [63, 64, 191, 192].contains(&v);
            assert_eq!(set.contains(v), expected, "mismatch at {v}");
        }
    }

    #[test]
    fn idempotent_insert() {
        let mut set = MmapBitset::new(128);
        set.insert(42);
        set.insert(42);
        assert!(set.contains(42));
        assert!(!set.contains(43));
    }

    #[test]
    fn spans_many_pages() {
        // 5_000_000 bits ~= 610 KiB, many 4 KiB pages; touch a sparse few.
        let mut set = MmapBitset::new(5_000_000);
        for &v in &[0usize, 4_999_999, 1_234_567, 2_000_000] {
            set.insert(v);
        }
        assert!(set.contains(0));
        assert!(set.contains(4_999_999));
        assert!(set.contains(1_234_567));
        assert!(set.contains(2_000_000));
        assert!(!set.contains(1_234_566));
        assert!(!set.contains(3_000_000));
    }

    #[test]
    fn tiny_universe_does_not_fail_mmap() {
        // num_nodes == 0 must still map one word rather than mmap(len=0).
        let set = MmapBitset::new(0);
        assert_eq!(set.words, 1);
    }
}
