# bfs_bench

Benchmarks the **visited-set data structure** used by breadth-first search on
large compressed graphs. During a BFS the only per-node state is "have I seen
this node?", and the data structure answering that question dominates the cost
of the traversal. This tool runs the same BFS with many candidate
representations and reports how long each takes, so you can pick the right one
for your graph and traversal shape.

Graphs are read in [WebGraph](https://webgraph.di.unimi.it/) `BVGraph` format
via [webgraph-rs](https://github.com/vigna/webgraph-rs).

## What it measures

For each graph it sweeps BFS depth limits `1, 2, 3, 4, 5` and unbounded
(`usize::MAX`) from four fixed roots, timing one BFS per visited-set
implementation (a warm-up run precedes each batch). The implementations:

| Implementation | Notes |
| --- | --- |
| `Vec<bool>` | One byte per node in the universe. |
| `BitVec` (`sux`) | One bit per node in the universe. |
| `AdaptiveNodeSet` | Starts as a `HashSet`, promotes to a dense `BitVec` once it gets dense. |
| `BlockBitset` | Two-level, lazily-allocated dense bitset (uncompressed Roaring). |
| `SparseSet` | Briggs-Torczon sparse set; universe-sized, only run when it fits in RAM. |
| `RoaringTreemap` (`roaring`) | Compressed bitmap. |
| `croaring::Bitmap64` (`croaring`) | Compressed bitmap (CRoaring bindings). |
| `SparseRadixSet32<H>` | Splits ids into a 32-bit bucket index + 32-bit stored low part; with `FxHasher` and `RapidHasher`. |
| `SparseRadixSet32<AdaptiveBucket>` | Same radix split, but each bucket is an `AdaptiveNodeSet` (sparse `HashSet`, promoting to a dense `BitVec` over its `2^32` sub-universe), so dense regions are capped per bucket. |
| `HashSet<usize, H>` | `std` hash set across many hashers: default, ahash, fxhash, wyhash, rapidhash, xxh3, foldhash, rustc-hash, nohash, and (optionally) gxhash. |
| `BTreeSet<usize>` | Ordered-tree baseline. |

`SparseSet` is skipped on graphs with more than ~3e9 nodes (it eagerly
allocates one word per node in the universe). `gxhash` requires a CPU with
AES/SSE2 intrinsics and is behind a Cargo feature (see below).

## Graphs

The data structures are evaluated on the
[Software Heritage](https://docs.softwareheritage.org/devel/swh-graph/) merkle
graph (tens of billions of nodes), which is the workload this benchmark exists
to inform: it runs huge numbers of small BFSs over a graph far too large to
hold uncompressed, so the visited-set choice matters. For smaller, public,
reproducible reference points we also run the
[LAW](http://law.di.unimi.it/datasets.php) datasets, e.g. `dblp-2010`,
`hollywood-2011`, `enwiki-2015`, `in-2004`, `webbase-2001`, `twitter-2010`,
`eu-2015`.

Any `BVGraph` works. A graph is referenced by its **basename** — the path with
the `.graph`/`.properties` extension stripped, e.g. pass `/data/graphs/dblp-2010`
for `dblp-2010.graph`. Random access additionally needs the Elias-Fano offsets
(`.ef`) file; see the
[webgraph-rs docs](https://github.com/vigna/webgraph-rs) for downloading LAW
graphs and building the `.ef` offsets.

## Build

With Nix (provides the pinned Rust toolchain plus the Python plotting deps):

```sh
nix develop
cargo build --release
```

Without Nix: a Rust toolchain (edition 2024, Rust >= 1.85) and `cargo build
--release`. The build links a few native dependencies (`croaring`, `openssl`,
`protobuf`, `zlib`); on non-Nix systems install those via your package manager.

## Run

```sh
# One or more graph basenames, benchmarked in the order given:
./target/release/bfs_bench /data/graphs/dblp-2010 /data/graphs/in-2004

# Memory-map graphs from disk instead of loading them into RAM
# (use for graphs too large to fit in memory):
./target/release/bfs_bench --mmap /data/graphs/swh-graph

./target/release/bfs_bench --help
```

By default each graph is loaded fully into RAM (`LoadMmap`), which gives the
cleanest timings. `--mmap` keeps the graph on disk (`Mmap`) for graphs that do
not fit in memory, at the cost of page-fault noise in the measurements.

Results are written to **stdout** as tab-separated rows; progress goes to
stderr. Redirect stdout to a file to capture them:

```sh
./target/release/bfs_bench /data/graphs/dblp-2010 > results.tsv
```

Each row is:

```
<max_depth>    <graph-basename>    <data-structure>    <nanoseconds>
```

where `<max_depth>` is `18446744073709551615` (`usize::MAX`) for the unbounded
runs.

## Plot

`plot.py` turns a results file into per-depth bar charts (median time relative
to the fastest structure, plus an average across graphs) and prints a summary
table.

```sh
# In the Nix dev shell the Python deps are already present; otherwise:
pip install -r requirements.txt

python plot.py results.tsv -o plots/
```

It writes `bfs_relative_performance_<depth>.png` and
`bfs_average_performance_<depth>.png` into the output directory (default: the
current directory); the unbounded depth is labelled `max`.

## Layout

- `src/main.rs` — CLI, the `NodeSet` trait, its implementations for `std`/third-party sets, and the benchmark loop.
- `src/adaptive_node_set.rs`, `src/block_bitset.rs`, `src/sparse_set.rs`, `src/sparse_radix_set.rs` — the custom visited-set implementations (with unit tests).
- `plot.py` — visualization.
- `flake.nix` — dev shell (Rust toolchain + Python plotting deps).

## Optional features

- `gxhash` — adds the `gxhash`-backed `HashSet` to the benchmark:
  ```sh
  cargo build --release --features gxhash
  ```
