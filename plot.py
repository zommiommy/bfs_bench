#!/usr/bin/env python3
"""Plot bfs_bench results.

Reads the tab-separated rows printed by `bfs_bench`:

    <max_depth>\t<graph-basename>\t<data-structure>\t<nanoseconds>

and writes, per BFS depth, a bar chart of each data structure's median time
relative to the fastest, plus an average-across-graphs chart.
"""

import argparse
from pathlib import Path

import matplotlib.pyplot as plt
import numpy as np
import pandas as pd
import seaborn as sns

# usize::MAX, printed by the Rust side for an unbounded (whole-graph) BFS.
UNBOUNDED_DEPTH = str(2**64 - 1)

# Substring of the fully-qualified Rust type name -> readable label. Ordered:
# the first matching substring wins, so more specific entries come first.
DS_LABELS = [
    ("SparseRadixSet32", {
        "FxHasher": "SparseRadixSet32 (FxHasher)",
        "RapidHasher": "SparseRadixSet32 (RapidHasher)",
    }, "SparseRadixSet32"),
    ("BlockBitset", None, "BlockBitset"),
    ("SparseSet", None, "SparseSet"),
    ("Bitmap64", None, "croaring (Bitmap64)"),
    ("foldhash", None, "HashSet (foldhash)"),
    ("rustc_hash", None, "HashSet (rustc-hash)"),
    ("NoHashHasher", None, "HashSet (nohash)"),
    ("gxhash", None, "HashSet (gxhash)"),
    ("HashSet<usize>", None, "HashSet (Default)"),
    ("FxHasher", None, "HashSet (FxHasher)"),
    ("AHasher", None, "HashSet (AHasher)"),
    ("WyHasherBuilder", None, "HashSet (WyHasher)"),
    ("Xxh3DefaultBuilder", None, "HashSet (Xxh3)"),
    ("RapidHasher", None, "HashSet (RapidHasher)"),
    ("Vec<bool>", None, "Vec<bool>"),
    ("BitVec", None, "BitVec"),
    ("BTreeSet<usize>", None, "BTreeSet<usize>"),
    ("RoaringTreemap", None, "RoaringTreemap"),
    ("AdaptiveNodeSet", None, "AdaptiveNodeSet"),
]


def readable_ds_name(ds_full_name):
    """Map a fully-qualified Rust type name to a short, readable label.

    Falls back to the last `::`-separated path component for unknown types so a
    newly added benchmark variant shows up instead of aborting the whole plot.
    """
    for needle, sub_map, default in DS_LABELS:
        if needle in ds_full_name:
            if sub_map:
                for sub_needle, label in sub_map.items():
                    if sub_needle in ds_full_name:
                        return label
            return default
    return ds_full_name.split("::")[-1]


def parse_data_file(filepath):
    """Parse a results file into {depth: {graph: {ds_name: np.array(times)}}}."""
    data = {}

    with open(filepath, "r") as file:
        for line in file:
            line = line.strip()
            if not line:
                continue

            parts = [p.strip() for p in line.split("\t")]
            if len(parts) < 4:
                continue

            max_depth = "max" if parts[0] == UNBOUNDED_DEPTH else parts[0]
            graph_name = parts[1].split("/")[-1]
            ds_name = readable_ds_name(parts[2])
            time = int(parts[-1])

            data.setdefault(max_depth, {}).setdefault(graph_name, {}).setdefault(
                ds_name, []
            ).append(time)

    return {
        depth: {
            graph_name: {ds_name: np.array(times) for ds_name, times in ds_data.items()}
            for graph_name, ds_data in graphs_data.items()
        }
        for depth, graphs_data in data.items()
    }


def build_palette(data):
    """Assign one stable color per data-structure label across every chart."""
    names = sorted(
        {
            ds_name
            for graphs_data in data.values()
            for ds_data in graphs_data.values()
            for ds_name in ds_data
        }
    )
    palette = sns.color_palette("muted", len(names))
    return {name: palette[i] for i, name in enumerate(names)}


def visualize_relative_performance(data, colors, outdir):
    """Per depth: a bar plot per graph of median time relative to the fastest."""
    plt.style.use("ggplot")
    sns.set_palette("muted")

    for max_depth, graphs_data in data.items():
        n_graphs = len(graphs_data)
        n_cols = min(3, n_graphs)
        n_rows = (n_graphs + n_cols - 1) // n_cols

        fig, axes = plt.subplots(n_rows, n_cols, figsize=(15, 4 * n_rows), squeeze=False)
        axes = axes.flatten()

        for i, (graph_name, ds_data) in enumerate(graphs_data.items()):
            sorted_items = sorted(ds_data.items(), key=lambda x: np.median(x[1]))
            ds_names = [item[0] for item in sorted_items]
            times = [item[1] for item in sorted_items]

            best_time = min(np.median(t) for t in times)
            relative_times = [np.median(t) / best_time for t in times]

            ax = axes[i]
            bars = ax.bar(
                range(len(ds_names)),
                relative_times,
                color=[colors[name] for name in ds_names],
            )

            ax.set_xticks(range(len(ds_names)))
            ax.set_xticklabels(ds_names, rotation=45, ha="right")

            for bar in bars:
                height = bar.get_height()
                ax.text(
                    bar.get_x() + bar.get_width() / 2.0,
                    height * 1.01,
                    f"{height:.2f}x",
                    ha="center",
                    va="bottom",
                    fontsize=8,
                )

            ax.set_title(f"Graph: {graph_name} MaxDepth={max_depth}", fontsize=10)
            ax.set_ylabel("Relative Time (lower is better)")
            ax.spines["top"].set_visible(False)
            ax.spines["right"].set_visible(False)
            ax.set_ylim(bottom=0)

        for i in range(n_graphs, len(axes)):
            fig.delaxes(axes[i])

        plt.tight_layout()
        out = outdir / f"bfs_relative_performance_{max_depth}.png"
        plt.savefig(out, dpi=300, bbox_inches="tight")
        plt.close(fig)
        print(f"wrote {out}")


def create_summary_table(data, colors, outdir):
    """Per depth: average relative performance across all graphs, printed + charted."""
    for max_depth, graphs_data in data.items():
        summary_data = {}
        for ds_data in graphs_data.values():
            best_time = min(np.median(t) for t in ds_data.values())
            for ds_name, time in ds_data.items():
                summary_data.setdefault(ds_name, []).append(np.median(time) / best_time)

        avg_ratios = {ds: np.mean(ratios) for ds, ratios in summary_data.items()}
        df_summary = pd.DataFrame(
            {
                "Data Structure": list(avg_ratios.keys()),
                "Average Relative Performance": list(avg_ratios.values()),
            }
        ).sort_values("Average Relative Performance")

        print(f"\nAverage relative performance (MaxDepth={max_depth}):")
        print(df_summary.to_string(index=False))

        plt.figure(figsize=(10, 6))
        bars = plt.bar(
            df_summary["Data Structure"],
            df_summary["Average Relative Performance"],
            color=[colors[name] for name in df_summary["Data Structure"]],
        )
        for bar in bars:
            height = bar.get_height()
            plt.text(
                bar.get_x() + bar.get_width() / 2.0,
                height * 1.01,
                f"{height:.2f}x",
                ha="center",
                va="bottom",
            )

        plt.title(f"Average Relative Performance Across All Graphs (MaxDepth={max_depth})")
        plt.ylabel("Average Relative Time (lower is better)")
        plt.xticks(rotation=45, ha="right")
        plt.tight_layout()
        out = outdir / f"bfs_average_performance_{max_depth}.png"
        plt.savefig(out, dpi=300, bbox_inches="tight")
        plt.close()
        print(f"wrote {out}")


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("results", help="path to a bfs_bench results file (tab-separated)")
    parser.add_argument(
        "-o",
        "--outdir",
        default=".",
        type=Path,
        help="directory for the generated PNGs (default: current directory)",
    )
    args = parser.parse_args()

    args.outdir.mkdir(parents=True, exist_ok=True)

    data = parse_data_file(args.results)
    if not data:
        parser.error(f"no parseable rows in {args.results}")

    colors = build_palette(data)
    visualize_relative_performance(data, colors, args.outdir)
    create_summary_table(data, colors, args.outdir)


if __name__ == "__main__":
    main()
