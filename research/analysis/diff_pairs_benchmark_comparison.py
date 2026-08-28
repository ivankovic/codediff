#  This file is part of the CodeDiff code diffing tool.
#
#  Copyright (C) 2026 Marko Ivankovic
#
#  This program is free software: you can redistribute it and/or modify
#  it under the terms of the GNU Affero General Public License as published
#  by the Free Software Foundation, either version 3 of the License, or
#  (at your option) any later version.
#
#  This program is distributed in the hope that it will be useful,
#  but WITHOUT ANY WARRANTY; without even the implied warranty of
#  MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
#  GNU Affero General Public License for more details.
#
#  You should have received a copy of the GNU Affero General Public License
#  along with this program.  If not, see <https://www.gnu.org/licenses/>.
"""Compare two `benchmark_diff_pairs` runs (e.g. before/after a diff_code algorithm change) over
the same sampled pairs and chart the difference.

Usage (from research/, matching the other analysis scripts' convention):
    uv run ./analysis/diff_pairs_benchmark_comparison.py
"""

import argparse
import csv
from collections import Counter
from pathlib import Path

import matplotlib.pyplot as plt
import numpy as np


def read_rows(csv_path):
    with open(csv_path, newline="") as f:
        return list(csv.DictReader(f))


def key(row):
    return (row["repository"], row["commit"], row["path"])


def join_ok_rows(before_rows, after_rows):
    """Pair up rows present (and status="ok") on both sides, keyed by (repository, commit, path)."""
    after_by_key = {key(r): r for r in after_rows}

    pairs = []
    for before_row in before_rows:
        after_row = after_by_key.get(key(before_row))
        if after_row is None:
            continue
        if before_row["status"] != "ok" or after_row["status"] != "ok":
            continue
        pairs.append((before_row, after_row))
    return pairs


def print_status_counts(label, rows):
    counts = Counter(r["status"] for r in rows)
    print(f"{label} status counts: " + ", ".join(f"{k}={v}" for k, v in sorted(counts.items())))


def print_timing_summary(label, elapsed_ms):
    arr = np.asarray(elapsed_ms, dtype=float)
    p50, p90, p99 = np.percentile(arr, [50, 90, 99])
    print(
        f"{label}: n={len(arr)}  total={arr.sum() / 1000:.1f}s  mean={arr.mean():.1f}ms  "
        f"p50={p50:.1f}ms  p90={p90:.1f}ms  p99={p99:.1f}ms  max={arr.max():.1f}ms"
    )


def plot_nodes_vs_time(pairs, output_path):
    """Scatter of combined AST size vs elapsed time, before and after, on shared log-log axes."""
    total_nodes = np.array(
        [int(b["ast_nodes_before"]) + int(b["ast_nodes_after"]) for b, _ in pairs]
    )
    before_ms = np.array([float(b["elapsed_ms"]) for b, _ in pairs])
    after_ms = np.array([float(a["elapsed_ms"]) for _, a in pairs])

    plt.figure(figsize=(9, 7))
    plt.scatter(
        total_nodes, before_ms, s=14, alpha=0.5, label="before (optimal_iud)", color="tab:red"
    )
    plt.scatter(
        total_nodes, after_ms, s=14, alpha=0.5, label="after (Zhang-Shasha)", color="tab:green"
    )
    plt.xscale("log")
    plt.yscale("log")
    plt.xlabel("Combined AST nodes (before + after, log scale)")
    plt.ylabel("diff_code elapsed time (ms, log scale)")
    plt.title("diff_code time vs input size: before vs after the optimal_iud fix")
    plt.legend()
    plt.grid(True, which="both", alpha=0.2)

    output_path.parent.mkdir(parents=True, exist_ok=True)
    plt.savefig(output_path, dpi=300, bbox_inches="tight")
    plt.close()
    print(f"Plot saved to {output_path}")


def plot_time_ecdf(pairs, output_path):
    """ECDF of elapsed_ms, before vs after, log-x. Shows the whole distribution shifting left."""
    before_ms = np.sort(np.array([float(b["elapsed_ms"]) for b, _ in pairs]))
    after_ms = np.sort(np.array([float(a["elapsed_ms"]) for _, a in pairs]))
    before_y = np.arange(1, len(before_ms) + 1) / len(before_ms)
    after_y = np.arange(1, len(after_ms) + 1) / len(after_ms)

    plt.figure(figsize=(9, 6))
    plt.step(before_ms, before_y, where="post", label="before (optimal_iud)", color="tab:red")
    plt.step(after_ms, after_y, where="post", label="after (Zhang-Shasha)", color="tab:green")
    plt.xscale("log")
    plt.xlabel("diff_code elapsed time (ms, log scale)")
    plt.ylabel("Fraction of pairs <= x")
    plt.title("CDF of diff_code time: before vs after the optimal_iud fix")
    plt.legend()
    plt.grid(True, which="both", alpha=0.2)

    output_path.parent.mkdir(parents=True, exist_ok=True)
    plt.savefig(output_path, dpi=300, bbox_inches="tight")
    plt.close()
    print(f"Plot saved to {output_path}")


def plot_speedup_histogram(pairs, output_path):
    """Histogram of per-pair speedup factor (before_ms / after_ms), log-x.

    Most pairs cluster near 1x (already fast before); the long right tail is the pathological
    cases the optimal_iud fix targeted.
    """
    speedups = np.array(
        [float(b["elapsed_ms"]) / max(float(a["elapsed_ms"]), 1e-6) for b, a in pairs]
    )

    plt.figure(figsize=(9, 6))
    bins = np.logspace(np.log10(max(speedups.min(), 0.1)), np.log10(speedups.max()), 40)
    plt.hist(speedups, bins=bins, edgecolor="black", alpha=0.8, color="tab:blue")
    plt.axvline(1.0, color="black", linestyle="--", linewidth=1, label="1x (no change)")
    plt.xscale("log")
    plt.xlabel("Speedup factor (before_ms / after_ms, log scale)")
    plt.ylabel("Number of pairs")
    plt.title("Per-pair speedup distribution from the optimal_iud fix")
    plt.legend()

    output_path.parent.mkdir(parents=True, exist_ok=True)
    plt.savefig(output_path, dpi=300, bbox_inches="tight")
    plt.close()
    print(f"Plot saved to {output_path}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(
        description="Compare two benchmark_diff_pairs runs and chart the speed difference."
    )
    parser.add_argument(
        "--before", required=True, help="CSV from an earlier benchmark_diff_pairs run"
    )
    parser.add_argument("--after", required=True, help="CSV from a later benchmark_diff_pairs run")
    parser.add_argument("--plots-dir", default="plots")
    args = parser.parse_args()

    before_rows = read_rows(Path(args.before))
    after_rows = read_rows(Path(args.after))

    print_status_counts("before", before_rows)
    print_status_counts("after", after_rows)
    print()

    pairs = join_ok_rows(before_rows, after_rows)
    print(
        f"Matched {len(pairs)} pairs with status=ok on both sides "
        f"(of {len(before_rows)} sampled pairs)"
    )
    print()

    print_timing_summary("before (optimal_iud)", [float(b["elapsed_ms"]) for b, _ in pairs])
    print_timing_summary("after  (Zhang-Shasha)", [float(a["elapsed_ms"]) for _, a in pairs])
    print()

    speedups = np.array(
        [float(b["elapsed_ms"]) / max(float(a["elapsed_ms"]), 1e-6) for b, a in pairs]
    )
    print(
        f"Speedup factor: mean={speedups.mean():.1f}x  median={np.median(speedups):.1f}x  "
        f"max={speedups.max():.1f}x  (>=2x: {(speedups >= 2).sum()} pairs, "
        f">=10x: {(speedups >= 10).sum()} pairs)"
    )

    for threshold_ms in (100, 400, 1000, 5000):
        before_over = sum(1 for b, _ in pairs if float(b["elapsed_ms"]) >= threshold_ms)
        after_over = sum(1 for _, a in pairs if float(a["elapsed_ms"]) >= threshold_ms)
        print(f"Pairs >= {threshold_ms}ms: before={before_over}  after={after_over}")

    plots_dir = Path(args.plots_dir)
    plot_nodes_vs_time(pairs, plots_dir / "diff_pairs_benchmark_nodes_vs_time.png")
    plot_time_ecdf(pairs, plots_dir / "diff_pairs_benchmark_time_ecdf.png")
    plot_speedup_histogram(pairs, plots_dir / "diff_pairs_benchmark_speedup_histogram.png")
