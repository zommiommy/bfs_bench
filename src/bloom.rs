use fastbloom::BloomFilter;
use fastbloom::BuilderWithBits;
use crate::NodeSet;
use crate::BuildableHasher;
use std::hash::RandomState;

pub struct Bloom<T: NodeSet, H: BuildableHasher = RandomState> {
    set: T,
    filter: BloomFilter<512, H>,
}

impl<T: NodeSet, H: BuildableHasher> NodeSet for Bloom<T, H> {
    fn new(num_nodes: usize) -> Self {
        Bloom {
            set: T::new(num_nodes),
            filter: BloomFilter::with_false_pos(0.01).hasher(H::new()).expected_items(num_nodes),
        }
    }

    #[inline(always)]
    fn insert(&mut self, node: usize) {
        self.set.insert(node);
        self.filter.insert(&node);
    }

    #[inline(always)]
    fn contains(&self, node: usize) -> bool {
        self.filter.contains(&(node as u64)) && self.set.contains(node)
    }
}

