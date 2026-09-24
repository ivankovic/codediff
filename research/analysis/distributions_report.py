#!/usr/bin/env python3
# This file is part of the CodeDiff code diffing tool.
#
# Copyright (C) 2026 Marko Ivankovic
#
# This program is free software: you can redistribute it and/or modify
# it under the terms of the GNU Affero General Public License as published
# by the Free Software Foundation, either version 3 of the License, or
# (at your option) any later version.
#
# This program is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
# GNU Affero General Public License for more details.
#
# You should have received a copy of the GNU Affero General Public License
# along with this program. If not, see <https://www.gnu.org/licenses/>.

"""Every size-or-time distribution the introductory paper reports, drawn in one idiom.

Until 2026-09-18 the paper summarised its distributions four different ways: p50/p90/p99/max
tables for file and edit sizes, a grouped bar chart for the whole-tree APTED budget, a violin for
tool runtime, and percentile tables again for tool speed. A reviewer asked for one coherent way to
read them. Every one of those is the same question - what share of the population sits at or
below x - so every one is drawn here as an empirical cumulative distribution on a log axis, with
the paper's own percentiles marked on the curve and, where a budget exists, the budget as a
vertical line whose crossing is the number the paper quotes.

Two figures:

* `corpus_shape`: six panels. File size (lines, bytes, AST nodes) over every code file in the
  corpus, and edit size (lines changed per file, per commit, share of the file rewritten) over
  every code-file modification in the corpus's history. Replaces the paper's Tables 1 and 2 as
  the primary presentation; the tables' numbers are still generated and still quoted in prose.
* `time_budget`: two panels sharing one axis and one budget line. Whole-tree APTED completion
  time per artifact category (RQ2; the curve's height at the budget *is* the completion rate,
  timeouts being the gap to 100%), and per-tool wall-clock over every repeated run (RQ4.2 and the
  CodeDiff speed paragraph). Replaces the runtime violin.

Inputs are the committed distribution files the producer scripts write (`file_stats.py`,
`edit_shape_stats.py`), the RQ2 CSVs, and `benchmark_other.csv`; nothing here measures anything.
"""

import argparse
import collections
import glob
from pathlib import Path

import matplotlib
import matplotlib.pyplot as plt
import numpy as np
from matplotlib import ticker

matplotlib.use("Agg")

from _common import (
    GRIDLINE,
    INK_MUTED,
    INK_PRIMARY,
    INK_SECONDARY,
    RESEARCH_DIR,
    SURFACE,
    read_rows,
)
from apted_only_report import (
    ATTEMPTED_STATUSES,
    CATEGORY_ORDER,
    CODE,
    CONFIG_DATA,
    LANGUAGE_CATEGORY,
    SCRIPTING,
)
from benchmark_other_report import COLORS, DISPLAY_NAMES, ordered, speed_sample

# One colour per artifact category, shared with apted_only_report's bar chart so the two agree.
CATEGORY_COLORS = {CODE: "#2a78d6", SCRIPTING: "#e08a3c", CONFIG_DATA: "#6d8a5a"}
CATEGORY_LABELS = {CODE: "Code", SCRIPTING: "Scripting", CONFIG_DATA: "Config / data"}

BUDGET_MS = 1000.0

PERCENTILES = (50, 90, 99)


# ── Empirical distributions from value->count rows ──────────────────────────────────────────────


def read_counts(path: Path, metric: str) -> tuple[np.ndarray, np.ndarray]:
    """`(values, counts)` sorted by value for `metric` in a value->count CSV. Any extra columns
    (per-file flags, say) are summed over: the paper's Table 1 filters on category alone."""
    acc: collections.Counter = collections.Counter()
    for r in read_rows(path):
        if r["metric"] == metric:
            acc[float(r["value"])] += int(r["count"])
    if not acc:
        raise SystemExit(f"{path}: no rows for metric {metric!r}")
    values = np.array(sorted(acc), dtype=float)
    counts = np.array([acc[v] for v in values], dtype=float)
    return values, counts


def ecdf(values: np.ndarray, counts: np.ndarray) -> np.ndarray:
    """Share of the population at or below each value - the y of the curve, in percent."""
    return np.cumsum(counts) / counts.sum() * 100


def percentile_of(values: np.ndarray, counts: np.ndarray, q: int) -> float:
    """Nearest-rank percentile from counts - the same definition `edit_shape_stats.percentile`
    uses on its sorted list, so a dot here lands on the number the table prints."""
    n = int(counts.sum())
    index = min(n - 1, max(0, round(q / 100 * (n - 1))))
    return float(values[np.searchsorted(np.cumsum(counts), index + 1)])


def compact(v: float) -> str:
    if v >= 1e6:
        return f"{v / 1e6:.3g}M"
    if v >= 1e3:
        return f"{v / 1e3:.3g}k"
    return f"{v:.3g}"


# ── Chrome ──────────────────────────────────────────────────────────────────────────────────────


def chrome(ax, ylabel: bool = True) -> None:
    ax.set_facecolor(SURFACE)
    ax.set_ylim(0, 100)
    ax.yaxis.set_major_locator(ticker.MultipleLocator(25))
    ax.yaxis.set_major_formatter(ticker.FuncFormatter(lambda v, _: f"{v:.0f}%"))
    ax.grid(axis="both", color=GRIDLINE, zorder=0)
    for spine in ("top", "right"):
        ax.spines[spine].set_visible(False)
    for spine in ("left", "bottom"):
        ax.spines[spine].set_color(INK_MUTED)
    ax.tick_params(colors=INK_SECONDARY, labelsize=8.5)
    if ylabel:
        ax.set_ylabel("Share at or below", fontsize=9, color=INK_SECONDARY)


def draw_ecdf(ax, values, counts, color, label=None, mark=True, log=True, **kw):
    """One cumulative curve with the paper's percentiles dotted on it."""
    y = ecdf(values, counts)
    x = np.maximum(values, 1.0) if log else values
    ax.step(x, y, where="post", color=color, linewidth=1.6, label=label, zorder=3, **kw)
    if not mark:
        return
    for q in PERCENTILES:
        v = percentile_of(values, counts, q)
        xv = max(v, 1.0) if log else v
        ax.plot([xv], [q], marker="o", markersize=3.2, color=color, zorder=4)
        # p50 and p90 label to the lower right of their dot; p99 sits near the top edge, so its
        # label goes to the lower left, clear of the curve's flat tail and the title.
        ax.annotate(
            f"p{q} {compact(v)}",
            (xv, q),
            xytext=(4, -9) if q < 99 else (-5, -10),
            textcoords="offset points",
            fontsize=7.2,
            color=INK_SECONDARY,
            ha="left" if q < 99 else "right",
            zorder=5,
        )
    ax.text(
        0.98,
        0.06,
        f"max {compact(float(values[-1]))}",
        transform=ax.transAxes,
        fontsize=7.2,
        color=INK_SECONDARY,
        ha="right",
        va="bottom",
        zorder=5,
    )


# ── Figure 1: corpus shape ──────────────────────────────────────────────────────────────────────


def plot_corpus_shape(file_sizes: Path, edit_sizes: Path, out: Path) -> None:
    fig, axes = plt.subplots(2, 3, figsize=(11, 5.6), facecolor=SURFACE)
    panels = [
        (axes[0][0], file_sizes, "lines_of_code", "Lines per code file", True),
        (axes[0][1], file_sizes, "bytes", "Bytes per code file", True),
        (axes[0][2], file_sizes, "ast_nodes", "AST nodes per code file", True),
        (axes[1][0], edit_sizes, "lines_per_file", "Lines changed per file edit", True),
        (axes[1][1], edit_sizes, "lines_per_commit", "Lines changed per commit", True),
        (axes[1][2], edit_sizes, "churn_permille", "Share of the file rewritten", False),
    ]
    for ax, path, metric, title, log in panels:
        values, counts = read_counts(path, metric)
        if metric == "churn_permille":
            values = values / 10.0  # permille -> percent
        chrome(ax, ylabel=ax is axes[0][0] or ax is axes[1][0])
        if log:
            ax.set_xscale("log")
            ax.xaxis.set_major_locator(ticker.LogLocator(base=10, numticks=8))
            ax.xaxis.set_major_formatter(ticker.FuncFormatter(lambda v, _: compact(v)))
        else:
            ax.set_xlim(0, 100)
            ax.xaxis.set_major_formatter(ticker.FuncFormatter(lambda v, _: f"{v:.0f}%"))
        draw_ecdf(ax, values, counts, INK_PRIMARY, log=log)
        ax.set_title(
            f"{title}  (n = {int(counts.sum()):,})",
            fontsize=9.5,
            color=INK_PRIMARY,
            loc="left",
            pad=8,
        )
    fig.tight_layout(h_pad=1.6, w_pad=1.2)
    _save(fig, out)


# ── Figure 2: the one-second budget ─────────────────────────────────────────────────────────────


def rq1_series(paths: list[Path]) -> dict[str, tuple[np.ndarray, int]]:
    """Per category: sorted completion times of the pairs that finished, and the total number of
    pairs attempted (`ATTEMPTED_STATUSES`, the same population `apted_only_report` rates over).
    A timeout or an out-of-memory abort has no elapsed time; they are the gap between the curve's
    height at the budget and 100%."""
    done: dict[str, list[float]] = collections.defaultdict(list)
    total: collections.Counter = collections.Counter()
    for p in paths:
        for r in read_rows(p):
            if r["status"] not in ATTEMPTED_STATUSES:
                continue
            cat = LANGUAGE_CATEGORY[r["language"]]
            total[cat] += 1
            if r["status"] == "ok":
                done[cat].append(float(r["elapsed_ms"]))
    return {c: (np.array(sorted(done[c])), total[c]) for c in CATEGORY_ORDER if total[c]}


def plot_time_budget(rq1_paths: list[Path], benchmark: Path, out: Path) -> None:
    fig, (ax1, ax2) = plt.subplots(
        1, 2, figsize=(11, 4.2), facecolor=SURFACE, gridspec_kw={"width_ratios": [1, 1.35]}
    )

    # Panel 1: whole-tree APTED against the budget, per artifact category.
    chrome(ax1)
    ax1.set_xscale("log")
    ax1.set_xlim(0.5, 2000)
    series = rq1_series(rq1_paths)
    for cat, (times, n) in series.items():
        y = np.arange(1, len(times) + 1) / n * 100
        x = np.maximum(times, 0.5)
        color = CATEGORY_COLORS[cat]
        # Extend the curve flat to the budget: nothing else completes before the kill.
        ax1.step(
            np.append(x, BUDGET_MS),
            np.append(y, y[-1] if len(y) else 0),
            where="post",
            color=color,
            linewidth=1.8,
            zorder=3,
            label=f"{CATEGORY_LABELS[cat]} (n = {n:,})",
        )
        reach = y[-1] if len(y) else 0.0
        ax1.annotate(
            f"{reach:.0f}%",
            (BUDGET_MS, reach),
            xytext=(4, -3),
            textcoords="offset points",
            fontsize=8,
            color=color,
            fontweight="bold",
            zorder=5,
        )
    ax1.axvline(BUDGET_MS, color=INK_PRIMARY, linewidth=1, linestyle=(0, (4, 3)), zorder=2)
    ax1.annotate(
        "1 s budget",
        (BUDGET_MS, 3),
        xytext=(-4, 0),
        textcoords="offset points",
        fontsize=8,
        color=INK_PRIMARY,
        ha="right",
        rotation=90,
        va="bottom",
    )
    ax1.xaxis.set_major_formatter(ticker.FuncFormatter(lambda v, _: compact(v)))
    ax1.set_xlabel("Whole-tree APTED wall-clock, ms", fontsize=9, color=INK_SECONDARY)
    ax1.set_title("Sampled file pairs completed (RQ2)", fontsize=9.5, color=INK_PRIMARY, loc="left")
    ax1.legend(frameon=False, fontsize=8, loc="upper left", labelcolor=INK_SECONDARY)

    # Panel 2: every tool configuration, every repeated run, one curve each.
    chrome(ax2, ylabel=False)
    ax2.set_xscale("log")
    rows = read_rows(benchmark)
    ids = ordered([c[: -len("_ms")] for c in rows[0] if c.endswith("_ms")])
    xmax = 1.0
    for id_ in ids:
        sample = np.array(sorted(speed_sample(rows, id_)))
        if not len(sample):
            continue
        xmax = max(xmax, float(sample[-1]))
        y = np.arange(1, len(sample) + 1) / len(sample) * 100
        style = {}
        if id_ == "treesitter_parse":
            style = {"linestyle": (0, (2, 2)), "linewidth": 1.2}
        elif id_ == "codediff":
            # Dash-dotted as well as heavier: the paper's own series has to be distinct from the
            # nine solid ones in greyscale, where weight alone does not separate it.
            style = {"linewidth": 2.4, "linestyle": (0, (6, 2, 1, 2))}
        else:
            style = {"linewidth": 1.4}
        ax2.step(
            np.maximum(sample, 0.05),
            y,
            where="post",
            color=COLORS[id_],
            zorder=4 if id_ == "codediff" else 3,
            label=DISPLAY_NAMES.get(id_, id_),
            **style,
        )
    ax2.set_xlim(0.05, xmax * 1.5)
    ax2.axvline(BUDGET_MS, color=INK_PRIMARY, linewidth=1, linestyle=(0, (4, 3)), zorder=2)
    ax2.annotate(
        "1 s budget",
        (BUDGET_MS, 3),
        xytext=(-4, 0),
        textcoords="offset points",
        fontsize=8,
        color=INK_PRIMARY,
        ha="right",
        rotation=90,
        va="bottom",
    )
    ax2.xaxis.set_major_formatter(ticker.FuncFormatter(lambda v, _: compact(v)))
    ax2.set_xlabel(
        "Wall-clock per diff, ms (every repeated run over every fixture)",
        fontsize=9,
        color=INK_SECONDARY,
    )
    ax2.set_title(
        "Diff tools on the fixture corpus (RQ4.2)", fontsize=9.5, color=INK_PRIMARY, loc="left"
    )
    ax2.legend(
        frameon=False,
        fontsize=7.6,
        loc="upper left",
        bbox_to_anchor=(1.01, 1.0),
        labelcolor=INK_SECONDARY,
        handlelength=1.8,
        borderaxespad=0,
    )

    fig.tight_layout(w_pad=1.6)
    _save(fig, out)


def _save(fig, out: Path) -> None:
    out.parent.mkdir(parents=True, exist_ok=True)
    fig.savefig(out.with_suffix(".pdf"), bbox_inches="tight", facecolor=SURFACE)
    fig.savefig(out.with_suffix(".png"), dpi=170, bbox_inches="tight", facecolor=SURFACE)
    print(f"Wrote {out.with_suffix('.pdf')} and .png")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--file-sizes",
        type=Path,
        default=RESEARCH_DIR / "data" / "corpus_stats" / "code_file_size_distribution.csv",
    )
    parser.add_argument(
        "--edit-sizes",
        type=Path,
        default=RESEARCH_DIR / "data" / "corpus_stats" / "edit_shape_distribution.csv",
    )
    parser.add_argument(
        "--rq1-glob", default=str(RESEARCH_DIR / "data" / "rq1" / "apted_only_group*.csv")
    )
    parser.add_argument(
        "--benchmark",
        type=Path,
        default=RESEARCH_DIR / "data" / "comparison" / "benchmark_other.csv",
    )
    parser.add_argument("--out-dir", type=Path, default=RESEARCH_DIR / "plots")
    args = parser.parse_args()

    plot_corpus_shape(args.file_sizes, args.edit_sizes, args.out_dir / "corpus_shape")
    plot_time_budget(
        sorted(Path(p) for p in glob.glob(args.rq1_glob)),
        args.benchmark,
        args.out_dir / "time_budget",
    )


if __name__ == "__main__":
    main()
