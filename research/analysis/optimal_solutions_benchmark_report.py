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
"""CodeDiff's own performance/accuracy characteristics across the optimal_solutions corpus - no
other tool involved (see benchmark_other_report.py for the codediff-vs-external-tools comparison,
which only covers line-level agreement, not this corpus's AST-node-level mismatch count).

Reads optimal_solutions_benchmark.csv (produced by
`cargo run --release --bin benchmark_optimal_solutions -- --csv`), one row per fixture with
`elapsed_ms` (wall-clock time for one `diff_code` call - see that binary's own `elapsed_ms_for`),
`total_nodes` (before+after combined AST node count, solved fixtures only - "solved" meaning a
`human_mapping.json` exists to compare against), `algorithm_cost` (codediff's own edit-script cost,
`codediff::diff::cost::diff_cost` - a size-of-the-diff proxy, present for every fixture including
"unsolved" ones), and `mismatches` (disagreement count against the human-authored mapping, solved
fixtures only).

Writes four plots:

  - optimal_solutions_runtime_histogram.png: distribution of elapsed_ms across the whole corpus,
    log-x bins (runtimes span several orders of magnitude, from single-digit ms to multi-second
    outliers on the largest real-world fixtures).
  - optimal_solutions_runtime_vs_nodes.png: scatter of elapsed_ms against total_nodes (solved
    fixtures only, since that's the only source of total_nodes today), log-log - shows how runtime
    scales with the size of the whole file.
  - optimal_solutions_runtime_vs_diff_size.png: scatter of elapsed_ms against algorithm_cost (every
    fixture, solved or not), log-log - shows how runtime scales with the size of the change itself,
    as opposed to the size of the whole file (the previous plot).
  - optimal_solutions_mismatches_vs_nodes.png: scatter of mismatch count against total_nodes
    (solved fixtures only), log-x/symlog-y (most fixtures have zero mismatches, which a plain log
    y-axis can't show at all) - whether bigger files also tend to have more disagreement with the
    human mapping, or accuracy holds steady regardless of size.

Usage (from research/):
    uv run ./analysis/optimal_solutions_benchmark_report.py
    uv run ./analysis/optimal_solutions_benchmark_report.py --csv optimal_solutions_benchmark.csv --plots-dir plots/
"""
import argparse
import csv
from pathlib import Path

import matplotlib.pyplot as plt
import matplotlib.ticker as ticker
import numpy as np

# Chart chrome, from the dataviz skill's reference palette (light mode) - same tokens
# benchmark_other_report.py/matching_reasons_report.py/diff_pairs_benchmark_comparison.py use, kept
# consistent across every PNG under research/plots/.
SURFACE = "#fcfcfb"
INK_PRIMARY = "#0b0b0b"
INK_SECONDARY = "#52514e"
INK_MUTED = "#898781"
GRIDLINE = "#e1e0d9"
BASELINE = "#c3c2b7"
# Same blue benchmark_other_report.py's COLORS["codediff"] uses - this report only ever plots
# codediff's own numbers, so every series shares that one color rather than needing a legend.
CODEDIFF_COLOR = "#2a78d6"


def read_rows(csv_path: Path) -> list[dict]:
    with open(csv_path, newline="") as f:
        return list(csv.DictReader(f))


def solved_rows(rows: list[dict]) -> list[dict]:
    """Rows with a real `human_mapping.json` (`human_unsolved` == "false") - the only ones with a
    real `total_nodes`/`mismatches` (`benchmark_optimal_solutions.rs` writes "-" for both on an
    unsolved row). `algorithm_cost`/`elapsed_ms` are computed unconditionally regardless of solved
    status, so the runtime histogram and the runtime-vs-diff-size scatter use the *full* corpus
    (`rows`) instead of this filtered set - only the two plots that need `total_nodes` call this."""
    return [r for r in rows if r["human_unsolved"] == "false"]


def style_axes(ax) -> None:
    """Shared chart chrome for every plot in this report - same tokens/spine treatment
    `benchmark_other_report.py` uses."""
    ax.set_facecolor(SURFACE)
    ax.tick_params(colors=INK_MUTED, labelsize=9)
    ax.grid(True, which="major", color=GRIDLINE, linewidth=0.8, zorder=0)
    ax.grid(True, which="minor", color=GRIDLINE, linewidth=0.4, zorder=0, alpha=0.6)
    for spine in ("top", "right"):
        ax.spines[spine].set_visible(False)
    ax.spines["left"].set_color(BASELINE)
    ax.spines["bottom"].set_color(BASELINE)


def plot_runtime_histogram(rows: list[dict], output_path: Path) -> None:
    """Log-x histogram of `elapsed_ms` across the whole corpus - one `diff_code` call per fixture,
    every fixture (solved or not, see `solved_rows`'s doc comment)."""
    elapsed = np.array([float(r["elapsed_ms"]) for r in rows], dtype=float)
    elapsed = elapsed[elapsed > 0]

    fig, ax = plt.subplots(figsize=(10, 6), facecolor=SURFACE)
    bins = np.logspace(np.log10(elapsed.min()), np.log10(elapsed.max()), 40)
    ax.hist(elapsed, bins=bins, color=CODEDIFF_COLOR, edgecolor=SURFACE, linewidth=0.6, zorder=3)
    ax.set_xscale("log")
    ax.xaxis.set_major_formatter(ticker.FuncFormatter(lambda v, _: f"{v:,.3g}"))
    ax.set_xlabel("diff_code elapsed time (ms, log scale)", fontsize=11, color=INK_SECONDARY)
    ax.set_ylabel("Number of fixtures", fontsize=11, color=INK_SECONDARY)
    ax.set_title(
        f"CodeDiff runtime distribution across the optimal_solutions corpus ({len(elapsed)} fixtures)",
        fontsize=13, color=INK_PRIMARY, loc="left", pad=12,
    )
    ax.yaxis.set_major_locator(ticker.MaxNLocator(integer=True))
    style_axes(ax)

    median = float(np.median(elapsed))
    ax.axvline(median, color=INK_PRIMARY, linestyle="--", linewidth=1.2, zorder=4)
    ax.text(
        median, ax.get_ylim()[1] * 0.97, f" median {median:.2f} ms", color=INK_PRIMARY,
        fontsize=9.5, va="top",
    )

    output_path.parent.mkdir(parents=True, exist_ok=True)
    fig.savefig(output_path, dpi=150, bbox_inches="tight", facecolor=SURFACE)
    plt.close(fig)
    print(f"Plot saved to {output_path}")


def plot_scatter(
    x: np.ndarray,
    y: np.ndarray,
    xlabel: str,
    ylabel: str,
    title: str,
    output_path: Path,
    yscale: str = "log",
) -> None:
    """Shared scatter builder for the three runtime/size/accuracy comparisons below - same styling,
    just different axes and data. `yscale="symlog"` (linear near zero, log further out) is what
    `plot_mismatches_vs_nodes` needs: most fixtures have zero mismatches, a value a plain log axis
    can't place at all."""
    fig, ax = plt.subplots(figsize=(9, 7), facecolor=SURFACE)
    ax.scatter(x, y, s=22, alpha=0.55, color=CODEDIFF_COLOR, edgecolor="none", zorder=3)
    ax.set_xscale("log")
    if yscale == "symlog":
        # `y` here (mismatch counts) is never negative, but matplotlib's symlog always mirrors the
        # linear region below zero too - clipped off here (rather than left to draw an empty,
        # misleading negative half down to -10^2) since there's nothing meaningful down there.
        ax.set_yscale("symlog", linthresh=1)
        ax.set_ylim(bottom=-0.5)
    else:
        ax.set_yscale("log")
    ax.set_xlabel(xlabel, fontsize=11, color=INK_SECONDARY)
    ax.set_ylabel(ylabel, fontsize=11, color=INK_SECONDARY)
    ax.set_title(title, fontsize=13, color=INK_PRIMARY, loc="left", pad=12)
    style_axes(ax)

    output_path.parent.mkdir(parents=True, exist_ok=True)
    fig.savefig(output_path, dpi=150, bbox_inches="tight", facecolor=SURFACE)
    plt.close(fig)
    print(f"Plot saved to {output_path}")


def plot_runtime_vs_nodes(rows: list[dict], output_path: Path) -> None:
    scoped = solved_rows(rows)
    nodes = np.array([int(r["total_nodes"]) for r in scoped], dtype=float)
    elapsed = np.array([float(r["elapsed_ms"]) for r in scoped], dtype=float)
    mask = (nodes > 0) & (elapsed > 0)
    plot_scatter(
        nodes[mask], elapsed[mask],
        "Combined AST nodes (before + after, log scale)",
        "diff_code elapsed time (ms, log scale)",
        f"Runtime vs input size ({int(mask.sum())} solved fixtures)",
        output_path,
    )


def plot_runtime_vs_diff_size(rows: list[dict], output_path: Path) -> None:
    cost = np.array([float(r["algorithm_cost"]) for r in rows], dtype=float)
    elapsed = np.array([float(r["elapsed_ms"]) for r in rows], dtype=float)
    mask = (cost > 0) & (elapsed > 0)
    plot_scatter(
        cost[mask], elapsed[mask],
        "CodeDiff's own edit-script cost (log scale)",
        "diff_code elapsed time (ms, log scale)",
        f"Runtime vs diff size ({int(mask.sum())} of {len(rows)} fixtures)",
        output_path,
    )


def plot_mismatches_vs_nodes(rows: list[dict], output_path: Path) -> None:
    scoped = solved_rows(rows)
    nodes = np.array([int(r["total_nodes"]) for r in scoped], dtype=float)
    mismatches = np.array([int(r["mismatches"]) for r in scoped], dtype=float)
    mask = nodes > 0
    zero_count = int((mismatches[mask] == 0).sum())
    plot_scatter(
        nodes[mask], mismatches[mask],
        "Combined AST nodes (before + after, log scale)",
        "Mismatches vs the human-authored mapping (0, then log scale)",
        f"Accuracy vs input size ({len(scoped)} solved fixtures - {zero_count} match exactly)",
        output_path,
        yscale="symlog",
    )


if __name__ == "__main__":
    parser = argparse.ArgumentParser(
        description="CodeDiff's own performance/accuracy characteristics from optimal_solutions_benchmark.csv."
    )
    parser.add_argument(
        "--csv", default="data/quality/optimal_solutions_benchmark.csv",
        help="Path to the benchmark CSV (default: optimal_solutions_benchmark.csv)",
    )
    parser.add_argument("--plots-dir", default="plots", help="Directory for output PNGs (default: plots/)")
    args = parser.parse_args()

    csv_path = Path(args.csv)
    if not csv_path.exists():
        print(f"No such file: {csv_path}")
        print("Run:  cargo run --release --bin benchmark_optimal_solutions -- --csv")
        raise SystemExit(1)

    rows = read_rows(csv_path)
    print(f"Loaded {csv_path}: {len(rows)} fixtures")

    scoped = solved_rows(rows)
    print(f"{len(scoped)} solved (have a human_mapping.json), {len(rows) - len(scoped)} unsolved")

    if "elapsed_ms" not in (rows[0] if rows else {}):
        print("This CSV has no 'elapsed_ms' column - regenerate it with a build that includes")
        print("benchmark_optimal_solutions.rs's elapsed_ms_for timing (see this script's own doc comment).")
        raise SystemExit(1)

    plots_dir = Path(args.plots_dir)
    plot_runtime_histogram(rows, plots_dir / "optimal_solutions_runtime_histogram.png")
    plot_runtime_vs_nodes(rows, plots_dir / "optimal_solutions_runtime_vs_nodes.png")
    plot_runtime_vs_diff_size(rows, plots_dir / "optimal_solutions_runtime_vs_diff_size.png")
    plot_mismatches_vs_nodes(rows, plots_dir / "optimal_solutions_mismatches_vs_nodes.png")
