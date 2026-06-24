#![deny(unconditional_recursion)]
use std::time::Instant;
use std::collections::{BTreeSet, HashSet, VecDeque};
use anyhow::Result;
use fxhash::FxBuildHasher;
use webgraph::prelude::*;
use sux::prelude::*;
use roaring::RoaringTreemap;

mod adaptive_node_set;
use adaptive_node_set::AdaptiveNodeSet;

mod bloom;
mod sparse_radix_set;
use sparse_radix_set::SparseRadixSet32;

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


fn bench_all<N: NodeSet>(graph: impl RandomAccessGraph, graph_path: &str, root: usize, max_depth: usize, warmup: bool) -> Result<()> {
    bench::<N>(&graph, graph_path, root, max_depth, warmup)?;
    //bench::<Bloom<N, FxBuildHasher>>(graph_path, root)?;
    //bench::<Bloom<N, BuildHasherDefault<ahash::AHasher>>>(graph_path, root)?;
    //bench::<Bloom<N>>(graph_path, root)?;
    Ok(())
}

fn all(graph: impl RandomAccessGraph, graph_path: &str) -> Result<()> {
    for depth in [1, 2, 3, 4, 5, usize::MAX] {
        for root in [
            1337,
            420,
            69,
            666,
        ] {

            eprintln!("\n\nGraph: {}, Root: {}, Depth: {}\n", graph_path, root, depth);

            // warmup runs
            bench_all::<AdaptiveNodeSet>(&graph, graph_path, root, depth, true)?;

            bench_all::<BitVec>(&graph, graph_path, root, depth, false)?;
            bench_all::<Vec<bool>>(&graph, graph_path, root, depth, false)?;
            bench_all::<AdaptiveNodeSet>(&graph, graph_path, root, depth, false)?;
            bench_all::<RoaringTreemap>(&graph, graph_path, root, depth, false)?;
            bench_all::<HashSet<usize, BuildHasherDefault<ahash::AHasher>>>(&graph, graph_path, root, depth, false)?;
            bench_all::<HashSet<usize, FxBuildHasher>>(&graph, graph_path, root, depth, false)?;
            bench_all::<HashSet<usize, wyhash::WyHasherBuilder>>(&graph, graph_path, root, depth, false)?;
            bench_all::<rapidhash::RapidHashSet<usize>>(&graph, graph_path, root, depth, false)?;
            bench_all::<HashSet<usize, xxhash_rust::xxh3::Xxh3DefaultBuilder>>(&graph, graph_path, root, depth, false)?;
            bench_all::<SparseRadixSet32<fxhash::FxHashSet<u32>>>(&graph, graph_path, root, depth, false)?;
            bench_all::<SparseRadixSet32<rapidhash::RapidHashSet<u32>>>(&graph, graph_path, root, depth, false)?;
            bench_all::<HashSet<usize>>(&graph, graph_path, root, depth, false)?;
            bench_all::<BTreeSet<usize>>(&graph, graph_path, root, depth, false)?;
            println!("");
        }
    }

    Ok(())
}

fn main() {
    loop {
        for graph_path in [
            "/dfd/graphs/dblp-2010",
            "/dfd/graphs/hollywood-2011",
            "/dfd/graphs/enwiki-2015",
            "/dfd/graphs/in-2004",
            "/dfd/graphs/webbase-2001",
            "/dfd/graphs/twitter-2010",
            "/dfd/graphs/eu-2015",
        ] {
            let graph = BvGraph::with_basename(graph_path)
                .mode::<LoadMmap>()
                .flags(MemoryFlags::TRANSPARENT_HUGE_PAGES | MemoryFlags::RANDOM_ACCESS)
                .load().unwrap();
            all(graph, graph_path).unwrap();
        }

        // This one is too big to load into memory, so we use Mmap mode
        let graph_path =  "/dfd/graphs/2024-12-06/graph";
        let graph = BvGraph::with_basename(graph_path)
            .mode::<Mmap>()
            .flags(MemoryFlags::TRANSPARENT_HUGE_PAGES | MemoryFlags::RANDOM_ACCESS)
            .load().unwrap();
        all(graph, graph_path).unwrap();
    }
}
