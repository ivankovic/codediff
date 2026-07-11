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
"""Summarise which ASTMappingReason pass matched how much of the diff.

Reads optimal_solutions_benchmark.csv (produced by
`cargo run --release --bin benchmark_optimal_solutions -- --csv`), which has one
row per fixture and one column per raw `ASTMappingReason` variant (the count of
matched/deleted/inserted node-pairs that pass attributed to itself). Writes two
plots:

  - matching_reason_totals.png: one bar per method, total node-pairs across
    every fixture (linear y-axis; counts span ~4 orders of magnitude, so the
    smaller bars are tiny slivers - each bar is value-labeled at the tip so the
    exact count stays legible even where the bar itself is barely visible).
  - matching_reason_share_by_fixture.png: a 0-100% stacked bar per fixture,
    showing each method's share of that fixture's matched node-pairs.

Rare/experimental reasons (FullMap, StructId, OptIDU, FlatSeq - each under 1% of
matched pairs in every run to date) are folded into "Other" so the categorical
palette stays at 8 slots; see palette.md's "9th series" rule.

Usage (from research/):
    uv run ./analysis/matching_reasons_report.py
    uv run ./analysis/matching_reasons_report.py --csv optimal_solutions_benchmark.csv --plots-dir plots/
"""
import argparse
import csv
from pathlib import Path

import matplotlib.pyplot as plt
import matplotlib.ticker as ticker
import numpy as np

# Chart chrome, from the dataviz skill's reference palette (light mode).
SURFACE = "#fcfcfb"
INK_PRIMARY = "#0b0b0b"
INK_SECONDARY = "#52514e"
INK_MUTED = "#898781"
GRIDLINE = "#e1e0d9"
BASELINE = "#c3c2b7"

# Category order is fixed (never re-sorted by value - "color follows the entity,
# never its rank"). Colors are assigned by semantic request, not palette.md's
# derived order: APTED red, the two identical-hash reasons as a dark/light green
# pair (same family, different shade), structural orange, moved purple, comment
# pink, bottom-up expansion yellow, Other grey. "Other" absorbs every raw
# ASTMappingReason column not named below.
CATEGORY_COLUMNS: list[tuple[str, str, str]] = [
    # (display label, hex color, raw CSV column)
    ("Identical hash", "#1a8a3c", "IdHash"),
    ("Identical hash (ancestor)", "#8fce8f", "IdHashAnc"),
    ("Structural (ancestor)", "#eb6834", "StructAnc"),
    ("APTED", "#e34948", "APTED"),
    ("Moved subtree", "#4a3aa7", "Moved"),
    ("Comment sibling", "#e87ba4", "Comment"),
    ("Bottom-up expansion", "#eda100", "BottomUp"),
]
OTHER_LABEL, OTHER_COLOR = "Other", "#9c9b95"
OTHER_COLUMNS = ["FullMap", "StructId", "OptIDU", "FlatSeq"]

MAX_LABEL_LEN = 16


def read_rows(csv_path: Path) -> list[dict]:
    with open(csv_path, newline="") as f:
        return list(csv.DictReader(f))


def category_totals(rows: list[dict]) -> tuple[list[str], list[str], list[float]]:
    """Returns (labels, colors, totals) across all rows, fixed order + Other last."""
    labels = [label for label, _, _ in CATEGORY_COLUMNS] + [OTHER_LABEL]
    colors = [color for _, color, _ in CATEGORY_COLUMNS] + [OTHER_COLOR]
    totals = []
    for _, _, col in CATEGORY_COLUMNS:
        totals.append(sum(int(row[col]) for row in rows))
    totals.append(sum(int(row[col]) for row in rows for col in OTHER_COLUMNS))
    return labels, colors, totals


def truncate_label(name: str, max_len: int = MAX_LABEL_LEN) -> str:
    return name if len(name) <= max_len else name[: max_len - 1] + "…"


def plot_totals(rows: list[dict], output_path: Path) -> None:
    """Simple bar chart: one bar per method, total node-pairs matched (log scale)."""
    labels, colors, totals = category_totals(rows)

    fig, ax = plt.subplots(figsize=(9, 5.5), facecolor=SURFACE)
    ax.set_facecolor(SURFACE)

    x = np.arange(len(labels))
    bars = ax.bar(x, totals, width=0.62, color=colors, edgecolor=SURFACE, linewidth=1.5, zorder=3)

    ax.yaxis.set_major_formatter(ticker.FuncFormatter(lambda v, _: f"{v:,.0f}"))
    headroom = max(totals) * 0.06 if totals else 1
    ax.set_ylim(0, max(totals) + headroom * 3)

    for bar, total in zip(bars, totals):
        ax.text(
            bar.get_x() + bar.get_width() / 2, bar.get_height() + headroom, f"{total:,}",
            ha="center", va="bottom", fontsize=9, color=INK_SECONDARY,
        )

    ax.set_xticks(x)
    ax.set_xticklabels(labels, rotation=30, ha="right", fontsize=9, color=INK_MUTED)
    ax.set_ylabel("Matched node-pairs", fontsize=11, color=INK_SECONDARY)
    ax.set_title(
        "Which pass matched how much of the diff",
        fontsize=13, color=INK_PRIMARY, loc="left", pad=12,
    )
    ax.tick_params(axis="y", colors=INK_MUTED, labelsize=9)
    ax.grid(axis="y", color=GRIDLINE, linewidth=1, zorder=0)
    for spine in ("top", "right", "left"):
        ax.spines[spine].set_visible(False)
    ax.spines["bottom"].set_color(BASELINE)

    output_path.parent.mkdir(parents=True, exist_ok=True)
    fig.savefig(output_path, dpi=150, bbox_inches="tight", facecolor=SURFACE)
    plt.close(fig)
    print(f"Plot saved to {output_path}")


def plot_share_by_fixture(rows: list[dict], output_path: Path) -> None:
    """0-100% stacked bar per fixture: each method's share of that fixture's matches."""
    names = [row["solution"] for row in rows]
    n = len(rows)

    fig_width = max(11.0, n * 0.28)
    fig, ax = plt.subplots(figsize=(fig_width, 7), facecolor=SURFACE)
    ax.set_facecolor(SURFACE)

    x = np.arange(n)
    all_columns = [col for _, _, col in CATEGORY_COLUMNS] + OTHER_COLUMNS
    row_totals = np.array(
        [sum(int(row[c]) for c in all_columns) for row in rows], dtype=float
    )

    bottom = np.zeros(n)
    for label, color, col in CATEGORY_COLUMNS + [(OTHER_LABEL, OTHER_COLOR, None)]:
        if col is not None:
            counts = np.array([int(row[col]) for row in rows], dtype=float)
        else:
            counts = np.array(
                [sum(int(row[c]) for c in OTHER_COLUMNS) for row in rows], dtype=float
            )
        pct = np.divide(counts, row_totals, out=np.zeros(n), where=row_totals > 0) * 100
        ax.bar(
            x, pct, bottom=bottom, width=0.8, color=color, edgecolor=SURFACE,
            linewidth=0.5, label=label, zorder=3,
        )
        bottom += pct

    ax.set_ylim(0, 100)
    ax.set_ylabel("Share of matched node-pairs (%)", fontsize=11, color=INK_SECONDARY)
    ax.set_title(
        "Matching-reason mix per fixture",
        fontsize=13, color=INK_PRIMARY, loc="left", pad=12,
    )
    ax.set_xticks(x)
    ax.set_xticklabels(
        [truncate_label(name) for name in names], rotation=90, fontsize=6.5, color=INK_MUTED,
    )
    ax.set_xlim(-0.6, n - 0.4)
    ax.tick_params(axis="y", colors=INK_MUTED, labelsize=9)
    ax.grid(axis="y", color=GRIDLINE, linewidth=1, zorder=0)
    for spine in ("top", "right", "left"):
        ax.spines[spine].set_visible(False)
    ax.spines["bottom"].set_color(BASELINE)

    ax.legend(
        loc="lower center", bbox_to_anchor=(0.5, 1.02), ncol=4, frameon=False,
        fontsize=9, labelcolor=INK_SECONDARY,
    )

    output_path.parent.mkdir(parents=True, exist_ok=True)
    fig.savefig(output_path, dpi=150, bbox_inches="tight", facecolor=SURFACE)
    plt.close(fig)
    print(f"Plot saved to {output_path}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(
        description="Summarise ASTMappingReason totals from optimal_solutions_benchmark.csv."
    )
    parser.add_argument(
        "--csv", default="optimal_solutions_benchmark.csv",
        help="Path to the benchmark CSV (default: optimal_solutions_benchmark.csv)",
    )
    parser.add_argument(
        "--plots-dir", default="plots",
        help="Directory for output PNGs (default: plots/)",
    )
    args = parser.parse_args()

    csv_path = Path(args.csv)
    if not csv_path.exists():
        print(f"No such file: {csv_path}")
        print("Run:  cargo run --release --bin benchmark_optimal_solutions -- --csv")
        raise SystemExit(1)

    rows = read_rows(csv_path)
    print(f"Loaded {csv_path}: {len(rows)} fixtures")

    plots_dir = Path(args.plots_dir)
    plot_totals(rows, plots_dir / "matching_reason_totals.png")
    plot_share_by_fixture(rows, plots_dir / "matching_reason_share_by_fixture.png")
