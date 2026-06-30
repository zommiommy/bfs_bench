#![deny(unconditional_recursion)]
use std::time::Instant;
use std::collections::{BTreeSet, HashSet, VecDeque};
use anyhow::{Context, Result};
use fxhash::FxBuildHasher;
use webgraph::prelude::*;
use sux::prelude::*;
use roaring::RoaringTreemap;

mod adaptive_node_set;
use adaptive_node_set::AdaptiveNodeSet;

mod sparse_radix_set;
use sparse_radix_set::SparseRadixSet32;
#[cfg(target_pointer_width = "64")]
use sparse_radix_set::AdaptiveBucket;
mod block_bitset;
use block_bitset::BlockBitset;
mod mmap_bitset;
use mmap_bitset::MmapBitset;
mod sparse_set;
use sparse_set::SparseSet;

use std::hash::{Hasher, BuildHasher, BuildHasherDefault, RandomState};

pub trait BuildableHasher: BuildHasher {
    fn new() -> Self;
}

impl BuildableHasher for RandomState {
    fn new() -> Self {
        RandomState::new()
    }
}

impl BuildableHasher for xxhash_rust::xxh3::Xxh3DefaultBuilder {
    fn new() -> Self {
        xxhash_rust::xxh3::Xxh3DefaultBuilder::new()
    }
}

impl BuildableHasher for wyhash::WyHasherBuilder {
    fn new() -> Self {
        wyhash::WyHasherBuilder::new(1337)
    }
}

impl BuildableHasher for foldhash::fast::RandomState {
    fn new() -> Self {
        Self::default()
    }
}

impl BuildableHasher for rustc_hash::FxBuildHasher {
    fn new() -> Self {
        Self
    }
}

#[cfg(feature = "gxhash")]
impl BuildableHasher for gxhash::GxBuildHasher {
    fn new() -> Self {
        Self::default()
    }
}

impl<H: Default + Hasher> BuildableHasher for BuildHasherDefault<H> {
    fn new() -> Self {
        Self::default()
    }
}

pub trait NodeSet {
    fn new(num_nodes: usize) -> Self;
    fn insert(&mut self, key: usize);
    fn contains(&self, key: usize) -> bool;
}

impl<S: BuildableHasher> NodeSet for HashSet<usize, S> {
    fn new(_num_nodes: usize) -> Self {
        HashSet::with_hasher(S::new())
    }

    #[inline(always)]
    fn insert(&mut self, node: usize) {
        HashSet::insert(self, node);
    }
    #[inline(always)]
    fn contains(&self, node: usize) -> bool {
        HashSet::contains(self, &node)
    }
}

impl NodeSet for BTreeSet<usize> {
    fn new(_num_nodes: usize) -> Self {
        BTreeSet::new()
    }

    #[inline(always)]
    fn insert(&mut self, node: usize) {
        <BTreeSet<usize>>::insert(self, node);
    }
    #[inline(always)]
    fn contains(&self, node: usize) -> bool {
        <BTreeSet<usize>>::contains(self, &node)
    }
}

impl NodeSet for BitVec {
    fn new(num_nodes: usize) -> Self {
        BitVec::new(num_nodes)
    }

    #[inline(always)]
    fn insert(&mut self, node: usize) {
        self.set(node, true);
    }
    #[inline(always)]
    fn contains(&self, node: usize) -> bool {
        self.get(node)
    }
}

impl NodeSet for Vec<bool> {
    fn new(num_nodes: usize) -> Self {
        vec![false; num_nodes]
    }

    #[inline(always)]
    fn insert(&mut self, node: usize) {
        self[node] = true;
    }
    #[inline(always)]
    fn contains(&self, node: usize) -> bool {
        self[node]
    }
}


impl NodeSet for RoaringTreemap {
    fn new(_num_nodes: usize) -> Self {
        RoaringTreemap::new()
    }

    #[inline(always)]
    fn insert(&mut self, node: usize) {
        self.insert(node as u64);
    }
    #[inline(always)]
    fn contains(&self, node: usize) -> bool {
        self.contains(node as u64)
    }
}

impl NodeSet for croaring::Bitmap64 {
    fn new(_num_nodes: usize) -> Self {
        croaring::Bitmap64::new()
    }

    #[inline(always)]
    fn insert(&mut self, node: usize) {
        self.add(u64::try_from(node).unwrap());
    }
    #[inline(always)]
    fn contains(&self, node: usize) -> bool {
        croaring::Bitmap64::contains(self, u64::try_from(node).unwrap())
    }
}


fn bench<N: NodeSet>(graph: impl RandomAccessGraph, graph_basename: &str, root: usize, max_depth: usize, warmup: bool) -> Result<()> {
    let name = core::any::type_name::<N>();

    let start = Instant::now();

    let num_nodes = graph.num_nodes();
    let mut seen = N::new(num_nodes);
    let mut queue = VecDeque::new();

    queue.push_back((0, root as _));
    seen.insert(root);

    while !queue.is_empty() {
        let (depth, current_node) = queue.pop_front().unwrap();
        if depth > max_depth {
            break;
        }
        for succ in graph.successors(current_node) {
            if !seen.contains(succ) {
                queue.push_back((depth + 1, succ));
                seen.insert(succ);
            }
        }
    }

    if !warmup {
        println!("{}\t{:<32}\t{:<120}\t{:>18}", max_depth, graph_basename, name, start.elapsed().as_nanos());
    }
    Ok(())
}


/// Runs every [`NodeSet`] implementation over a fixed sweep of BFS depths and
/// root nodes on `graph`, printing one timing line per (depth, root, set).
fn all(graph: impl RandomAccessGraph, graph_path: &str) -> Result<()> {
    for depth in [1, 2, 3, 4, 5, usize::MAX] {
        for root in [1337, 420, 69, 666] {
            // Roots are fixed across graphs; skip any that fall outside this
            // graph so smaller graphs don't panic in successors()/insert().
            if root >= graph.num_nodes() {
                eprintln!("skipping root {root} on {graph_path}: only {} nodes", graph.num_nodes());
                continue;
            }

            eprintln!("\n\nGraph: {}, Root: {}, Depth: {}\n", graph_path, root, depth);

            // Warm up caches/branch predictors with one untimed run.
            bench::<AdaptiveNodeSet>(&graph, graph_path, root, depth, true)?;

            bench::<BitVec>(&graph, graph_path, root, depth, false)?;
            bench::<Vec<bool>>(&graph, graph_path, root, depth, false)?;
            bench::<AdaptiveNodeSet>(&graph, graph_path, root, depth, false)?;
            bench::<RoaringTreemap>(&graph, graph_path, root, depth, false)?;
            bench::<HashSet<usize, BuildHasherDefault<ahash::AHasher>>>(&graph, graph_path, root, depth, false)?;
            bench::<HashSet<usize, FxBuildHasher>>(&graph, graph_path, root, depth, false)?;
            bench::<HashSet<usize, wyhash::WyHasherBuilder>>(&graph, graph_path, root, depth, false)?;
            bench::<rapidhash::RapidHashSet<usize>>(&graph, graph_path, root, depth, false)?;
            bench::<HashSet<usize, xxhash_rust::xxh3::Xxh3DefaultBuilder>>(&graph, graph_path, root, depth, false)?;
            bench::<SparseRadixSet32<fxhash::FxHashSet<u32>>>(&graph, graph_path, root, depth, false)?;
            bench::<SparseRadixSet32<rapidhash::RapidHashSet<u32>>>(&graph, graph_path, root, depth, false)?;
            #[cfg(target_pointer_width = "64")]
            bench::<SparseRadixSet32<AdaptiveBucket>>(&graph, graph_path, root, depth, false)?;
            bench::<BlockBitset>(&graph, graph_path, root, depth, false)?;
            bench::<MmapBitset>(&graph, graph_path, root, depth, false)?;
            bench::<croaring::Bitmap64>(&graph, graph_path, root, depth, false)?;
            bench::<HashSet<usize, foldhash::fast::RandomState>>(&graph, graph_path, root, depth, false)?;
            bench::<HashSet<usize, rustc_hash::FxBuildHasher>>(&graph, graph_path, root, depth, false)?;
            bench::<HashSet<usize, nohash_hasher::BuildNoHashHasher<usize>>>(&graph, graph_path, root, depth, false)?;
            #[cfg(feature = "gxhash")]
            bench::<HashSet<usize, gxhash::GxBuildHasher>>(&graph, graph_path, root, depth, false)?;
            // SparseSet eagerly allocates ~16 bytes per node in the universe in
            // new(), so only run it where that comfortably fits in RAM.
            if graph.num_nodes() <= 3_000_000_000 {
                bench::<SparseSet>(&graph, graph_path, root, depth, false)?;
            }
            bench::<HashSet<usize>>(&graph, graph_path, root, depth, false)?;
            bench::<BTreeSet<usize>>(&graph, graph_path, root, depth, false)?;
            println!();
        }
    }

    Ok(())
}

fn main() -> Result<()> {
    let mut mmap = false;
    let mut basenames = Vec::new();
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "-h" | "--help" => {
                usage();
                return Ok(());
            }
            // Memory-map the graph from disk instead of loading it into RAM.
            // Use for graphs that do not fit in memory; otherwise the default
            // (load into RAM) gives cleaner timings free of page-fault noise.
            "-m" | "--mmap" => mmap = true,
            unknown if unknown.starts_with('-') => {
                eprintln!("error: unknown option '{unknown}'\n");
                usage();
                std::process::exit(2);
            }
            _ => basenames.push(arg),
        }
    }

    if basenames.is_empty() {
        usage();
        std::process::exit(2);
    }

    for basename in &basenames {
        let loader = BvGraph::with_basename(basename)
            .flags(MemoryFlags::TRANSPARENT_HUGE_PAGES | MemoryFlags::RANDOM_ACCESS);
        if mmap {
            let graph = loader.mode::<Mmap>().load()
                .with_context(|| format!("loading {basename} (mmap)"))?;
            all(graph, basename)?;
        } else {
            let graph = loader.mode::<LoadMmap>().load()
                .with_context(|| format!("loading {basename}"))?;
            all(graph, basename)?;
        }
    }

    Ok(())
}

fn usage() {
    eprintln!(
        "Usage: bfs_bench [--mmap] <graph-basename>...\n\
         \n\
         Benchmarks BFS visited-set data structures over each graph, in order.\n\
         A graph basename is the path without the .graph/.properties extension,\n\
         e.g. /data/graphs/dblp-2010 for dblp-2010.graph.\n\
         \n\
         Options:\n\
         \x20 -m, --mmap   Memory-map graphs from disk instead of loading into RAM\n\
         \x20             (use for graphs too large to fit in memory).\n\
         \x20 -h, --help   Show this help.\n\
         \n\
         Results are printed to stdout as tab-separated rows:\n\
         \x20 <max_depth> <graph-basename> <data-structure> <nanoseconds>"
    );
}
