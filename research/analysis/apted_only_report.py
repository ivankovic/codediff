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
"""RQ1 report: "What percentage of real-world source-code changes can a single, whole-tree
tree-edit-distance computation complete within a one-second budget?"

Reads apted_only_benchmark's per-pair CSV output(s) (language, size_bucket, repository, commit,
path, loc_before, loc_after, loc_combined, bytes_before, bytes_after, ast_nodes_before,
ast_nodes_after, status, elapsed_ms), buckets every pair by loc_combined, and reports/plots the
percentage with status "ok" (finished inside the 1s budget the driver enforced via subprocess
kill, not this script) per bucket.

The measured algorithm is CodeDiff's own APTED implementation (`apted::for_roots`,
`Algorithm::Apted`), run directly on the whole before/after trees with none of CodeDiff's 7-phase
pipeline's pre-matching heuristics applied first - not a generic/stock APTED implementation. This
implementation includes CodeDiff's own containment-aware `compute_delta` optimization (2026-07-10,
~35% faster than the naive version on this project's own benchmark suite), so if anything it is
faster than a textbook implementation would be. The percentages this script reports are therefore
a lower bound on how often a whole-tree tree-edit-distance computation fails a one-second budget in
practice, not an upper bound - a stock/unoptimized implementation would do no better.

Two caveats the numbers below carry, both worth reading before citing a single headline percentage:

1. **The corpus is a size-stratified sample, not a uniform one.** `sample_code_pairs.rs` samples
   per language *per size bucket* (small/medium/large/xlarge, by byte size) specifically so that
   large files "aren't drowned out by the much more common small ones" (its own comment) - i.e. it
   deliberately over-represents the expensive tail relative to the true population of real commits
   (compare Table 1's file-size percentiles or Arafat and Riehle's 47.5%-of-commits-under-10-lines
   finding). This means the single aggregate "Overall: X% processed" figure below is a statement
   about *this sample*, not a population estimate - it is biased low relative to what a uniformly
   sampled corpus of real commits would show, because it contains proportionally far more large
   pairs than reality does. The **per-bucket** percentages do not have this problem: each one is a
   conditional rate ("given a change this size, how often does APTED finish in 1s"), which
   stratified sampling does not bias, only the relative sample *counts* per bucket - which is
   exactly why this report's primary evidence is the bucketed chart, not the aggregate number.

2. **LOC is a proxy, not the true cost driver.** APTED's cost is governed by AST node count and
   tree shape (depth, leaf/internal ratio), not lines of code, and LOC-per-node varies widely by
   language and file kind (a JSON translation list is thousands of near-identical shallow lines; a
   dense C++ header is far fewer lines but a deeper, more irregular tree). Re-bucketing this same
   corpus by combined AST node count instead of LOC reproduces the same non-monotonic shape the LOC
   chart shows (see `describe_quantiles`'s node-bucket printout below) - confirming the dip in the
   middle buckets is a real property of which languages/tree shapes populate that size range, not
   an artifact of using LOC as the x-axis. LOC is kept as the x-axis here because it is the
   quantity practitioners reason about, not because it is the more precise predictor.

Usage (from research/):
    uv run ./analysis/apted_only_report.py
    uv run ./analysis/apted_only_report.py --results 'results/apted_only_group*.csv' --plots-dir plots/
"""

import argparse
import csv
import glob
from pathlib import Path

import matplotlib.pyplot as plt
import matplotlib.ticker as ticker
import numpy as np

# Same chart-chrome tokens as benchmark_other_report.py / matching_reasons_report.py, from the
# dataviz skill's reference palette (light mode) - kept identical across every research/plots/*.png
# so the paper's figures share one visual language.
SURFACE = "#fcfcfb"
INK_PRIMARY = "#0b0b0b"
INK_SECONDARY = "#52514e"
INK_MUTED = "#898781"
GRIDLINE = "#e1e0d9"
BAR_COLOR = "#2a78d6"  # matches benchmark_other_report.py's "codediff" series color

# Combined (before + after) LOC bucket edges. 7 buckets, right-open except the last. Deliberately
# round numbers chosen after inspecting this corpus's actual loc_combined quantiles (printed by
# this script's own --describe-only diagnostic) rather than picked blind - see this file's git
# history / the introductory-paper README for the run that produced them. A bucket with too few
# pairs makes its percentage swing on one input; re-tune these if a re-sampled corpus shifts the
# distribution enough to hollow out a bucket.
BIN_EDGES = [0, 100, 300, 1000, 3000, 10000, 30000, float("inf")]


def bucket_label(lo: float, hi: float) -> str:
    if hi == float("inf"):
        return f"{lo:,.0f}+"
    return f"{lo:,.0f}–{hi:,.0f}"


def read_rows(paths: list[Path]) -> list[dict]:
    rows = []
    for path in paths:
        with open(path, newline="") as f:
            rows.extend(csv.DictReader(f))
    return rows


def describe_quantiles(loc_combined: np.ndarray) -> None:
    qs = [0, 10, 25, 50, 75, 90, 95, 99, 100]
    print("loc_combined quantiles (all attempted pairs):")
    for q in qs:
        print(f"  p{q:<3d} {np.percentile(loc_combined, q):>10,.0f}")


# Combined AST-node bucket edges, used only for the cross-check printed by `node_cross_check` -
# not plotted, and not the same edges as BIN_EDGES (chosen independently from node-count quantiles
# rather than reusing the LOC edges, since bytes/nodes and LOC are correlated but not identical -
# see the module docstring's caveat 2).
NODE_BIN_EDGES = [0, 200, 600, 2000, 6000, 20000, 60000, float("inf")]


def node_cross_check(attempted: list[dict], ok: np.ndarray) -> None:
    """Re-buckets the same `attempted` pairs by combined AST node count instead of loc_combined,
    printed (not plotted) so a reader can confirm the LOC chart's non-monotonic middle buckets
    are a real property of which languages/tree shapes populate that size range - not an artifact
    of choosing LOC, specifically, as the bucketing variable. See module docstring caveat 2."""
    nodes = np.array(
        [int(r["ast_nodes_before"]) + int(r["ast_nodes_after"]) for r in attempted], dtype=float
    )
    bucket_idx = np.digitize(nodes, NODE_BIN_EDGES[1:-1], right=True)
    print("\nCross-check: same pairs, bucketed by combined AST nodes instead of LOC:")
    for i in range(len(NODE_BIN_EDGES) - 1):
        mask = bucket_idx == i
        n = int(mask.sum())
        if n == 0:
            continue
        n_ok = int(ok[mask].sum())
        label = bucket_label(NODE_BIN_EDGES[i], NODE_BIN_EDGES[i + 1])
        print(f"  {label:>15} nodes n={n:<6} ok={n_ok:<6} ({100.0 * n_ok / n:5.1f}% processed within 1s)")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--results", default="results/apted_only_group*.csv", help="glob pattern for apted_only_benchmark output CSVs")
    parser.add_argument("--plots-dir", default="plots", type=Path)
    parser.add_argument("--output-png", default="apted_only_rq1.png")
    parser.add_argument("--describe-only", action="store_true", help="print loc_combined quantiles and exit, without plotting")
    args = parser.parse_args()

    paths = sorted(Path(p) for p in glob.glob(args.results))
    if not paths:
        raise SystemExit(f"no files matched {args.results!r}")
    rows = read_rows(paths)
    print(f"Loaded {len(rows)} rows from {len(paths)} file(s): {[p.name for p in paths]}")

    status_counts: dict[str, int] = {}
    for r in rows:
        status_counts[r["status"]] = status_counts.get(r["status"], 0) + 1
    print("Status counts:", status_counts)

    # RQ1's denominator: pairs APTED was actually attempted on. "parse_failed" (tree-sitter
    # couldn't parse one side at all) never reaches the timing question this RQ asks, so it's
    # excluded from the percentage and reported separately, not folded in as a failure of APTED.
    attempted = [r for r in rows if r["status"] in ("ok", "timed_out")]
    excluded = len(rows) - len(attempted)
    if excluded:
        print(f"Excluded {excluded} row(s) with status outside {{ok, timed_out}} (e.g. parse_failed) from the percentage below.")

    loc_combined = np.array([int(r["loc_combined"]) for r in attempted], dtype=float)
    describe_quantiles(loc_combined)

    if args.describe_only:
        return

    ok = np.array([r["status"] == "ok" for r in attempted], dtype=bool)

    bucket_idx = np.digitize(loc_combined, BIN_EDGES[1:-1], right=True)
    n_buckets = len(BIN_EDGES) - 1

    labels, pct_ok, ns = [], [], []
    print("\nBucket results:")
    for i in range(n_buckets):
        mask = bucket_idx == i
        n = int(mask.sum())
        n_ok = int(ok[mask].sum())
        pct = 100.0 * n_ok / n if n > 0 else 0.0
        label = bucket_label(BIN_EDGES[i], BIN_EDGES[i + 1])
        labels.append(label)
        pct_ok.append(pct)
        ns.append(n)
        print(f"  {label:>15} LOC   n={n:<6} ok={n_ok:<6} ({pct:5.1f}% processed within 1s)")

    overall_pct = 100.0 * ok.sum() / len(ok) if len(ok) else 0.0
    print(
        f"\nSample-weighted overall: {ok.sum()}/{len(ok)} ({overall_pct:.1f}%) processed within "
        "1s - NOT a population estimate, since this corpus is size-stratified per language and "
        "deliberately over-represents large files relative to real-world commits (see module "
        "docstring caveat 1). The per-bucket percentages above are the number to cite; this one "
        "is a summary of the sample actually measured, not of \"real-world file changes\" as a "
        "whole."
    )

    node_cross_check(attempted, ok)

    args.plots_dir.mkdir(parents=True, exist_ok=True)
    fig, ax = plt.subplots(figsize=(9, 5.5), facecolor=SURFACE)
    ax.set_facecolor(SURFACE)

    x = np.arange(n_buckets)
    bars = ax.bar(x, pct_ok, color=BAR_COLOR, edgecolor=SURFACE, linewidth=1, zorder=3, width=0.65)

    for bar, pct, n in zip(bars, pct_ok, ns):
        ax.text(
            bar.get_x() + bar.get_width() / 2, bar.get_height() + 1.5,
            f"{pct:.0f}%\n(n={n})", ha="center", va="bottom", fontsize=9, color=INK_SECONDARY, zorder=4,
        )

    ax.set_ylim(0, 108)
    ax.set_xticks(x)
    ax.set_xticklabels([f"{l}\nLOC" for l in labels], fontsize=9.5, color=INK_PRIMARY)
    ax.yaxis.set_major_formatter(ticker.FuncFormatter(lambda v, _: f"{v:.0f}%"))
    ax.set_ylabel("Pairs processed within 1s", fontsize=10.5, color=INK_PRIMARY)
    ax.set_xlabel("Combined before + after lines of code", fontsize=10.5, color=INK_PRIMARY)
    ax.set_title(
        "RQ1: whole-tree tree-edit distance vs. a 1-second budget\n"
        f"({len(attempted):,} real-world file changes, size-stratified per language - "
        "bars are within-bucket rates, not population-weighted)",
        fontsize=10.5, color=INK_PRIMARY,
    )
    ax.grid(axis="y", color=GRIDLINE, zorder=0)
    for spine in ("top", "right"):
        ax.spines[spine].set_visible(False)
    for spine in ("left", "bottom"):
        ax.spines[spine].set_color(INK_MUTED)

    output_path = args.plots_dir / args.output_png
    fig.savefig(output_path, dpi=150, bbox_inches="tight", facecolor=SURFACE)
    print(f"\nWrote {output_path}")


if __name__ == "__main__":
    main()
