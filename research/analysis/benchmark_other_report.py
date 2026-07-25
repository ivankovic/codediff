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
"""Compare codediff against other diff tools (Unix `diff` today - `benchmark_other.rs`'s
`ExternalTool` can grow more) on line-level agreement with the human-authored mapping, and on
runtime.

Reads benchmark_other.csv (produced by
`cargo run --release --bin benchmark_other -- --csv`), one row per fixture with
`<tool>_mismatches`/`<tool>_ms` columns for codediff and every `ExternalTool` - see that binary's
own doc comment for what "mismatch" means (a line where the tool's touched/untouched call
disagrees with the human mapping projected down to lines - the only signal a line-based tool like
Unix diff can produce at all, so it's the fairest common ground, not node-level accuracy).

Writes two plots:

  - benchmark_other_accuracy.png: grouped histogram of *agreement* rate (100 - mismatch rate, so
    higher is better and 100% means every line matched the human mapping), bucketed into fixed
    10-point-wide bins spanning the full 0-100% axis - one color per tool, sharing the same bins,
    so the two distributions sit side by side within each bucket.
  - benchmark_other_runtime.png: full per-fixture runtime distribution, one violin per tool plus a
    `treesitter_parse_ms` reference violin (log-scale y-axis, KDE computed in log10-space -
    codediff's per-fixture times span ~3 orders of magnitude, so a single mean bar hides exactly
    the shape that matters here). The reference violin is tree-sitter parsing alone, with no
    diffing on top - the lower bound every other series must clear before its own work starts.

Usage (from research/):
    uv run ./analysis/benchmark_other_report.py
    uv run ./analysis/benchmark_other_report.py --csv benchmark_other.csv --plots-dir plots/
"""
import argparse
import csv
from pathlib import Path

import matplotlib.pyplot as plt
import matplotlib.ticker as ticker
import numpy as np

# Chart chrome, from the dataviz skill's reference palette (light mode) - same tokens
# matching_reasons_report.py and diff_pairs_benchmark_comparison.py use.
SURFACE = "#fcfcfb"
INK_PRIMARY = "#0b0b0b"
INK_SECONDARY = "#52514e"
INK_MUTED = "#898781"
GRIDLINE = "#e1e0d9"
BASELINE = "#c3c2b7"

# Categorical slots 1 (blue), 2 (orange), 7 (violet), 6 (green), 8 (red) from the dataviz skill's
# reference palette - validated colorblind-safe as a fivesome via validate_palette.js (light + dark
# mode, all PASS) *in this exact left-to-right order* (green, blue, orange, violet, red) - the
# validator checks adjacent pairs, so re-ordering the x-axis without re-validating could silently
# reintroduce a CVD collision (green next to orange, tried first, failed at ΔE 3.2 protan). Each
# series keeps one fixed slot regardless of which subset a given chart shows - "color follows the
# entity, never its rank" - so `gumtree` is always violet and `gumtree_warm` is always red, whether
# plotted alongside every other series or alone.
CODEDIFF_COLOR = "#2a78d6"
TOOL_COLORS = {"unix_diff": "#eb6834", "gumtree": "#4a3aa7"}
TREESITTER_COLOR = "#008300"
GUMTREE_WARM_COLOR = "#e34948"


def read_rows(csv_path: Path) -> tuple[list[str], list[dict]]:
    with open(csv_path, newline="") as f:
        reader = csv.DictReader(f)
        rows = list(reader)
        return reader.fieldnames or [], rows


def tool_names(fieldnames: list[str]) -> list[str]:
    """Every `ExternalTool` present in the CSV, derived from its `<tool>_mismatches` column -
    `benchmark_other.rs`'s `ExternalTool::ALL` is the source of truth for which tools exist, so
    this just mirrors whatever that produced rather than hardcoding "unix_diff"."""
    return [c[: -len("_mismatches")] for c in fieldnames if c.endswith("_mismatches") and c != "codediff_mismatches"]


def pct(mismatches: np.ndarray, total: np.ndarray) -> np.ndarray:
    return np.divide(mismatches * 100, total, out=np.zeros_like(mismatches, dtype=float), where=total > 0)


def applicable_rows(rows: list[dict], tool: str) -> list[dict]:
    """Rows where `tool` was actually scored - `benchmark_other.rs` leaves `<tool>_mismatches`/
    `<tool>_ms` blank (not "0") for a fixture outside that tool's language scope
    (`ExternalTool::supports`), e.g. GumTree's Java-only scope leaves every non-Java fixture
    blank. Blending those in as zeros would make a narrowly-scoped tool look artificially
    perfect; dropping the rows entirely (not coercing to 0) is what every plot here does."""
    return [r for r in rows if r[f"{tool}_mismatches"] != ""]


def agreement(rows: list[dict], tool: str) -> np.ndarray:
    """Per-fixture agreement % for `tool`, filtered to `applicable_rows` internally - callers
    don't need to pre-filter `rows` per label themselves, since different labels in the same call
    (e.g. `plot_accuracy`'s "codediff" alongside "gumtree") can have different applicable subsets."""
    scoped = applicable_rows(rows, tool)
    total = np.array([int(r["total_lines"]) for r in scoped], dtype=float)
    mismatches = np.array([int(r[f"{tool}_mismatches"]) for r in scoped], dtype=float)
    return 100 - pct(mismatches, total)


def _plot_agreement_histogram(
    ax, panel_rows: list[dict], labels: list[str], colors: list[str], title: str, legend_labels: list[str] | None = None,
) -> None:
    """One bucketed-agreement histogram panel - factored out so a future tool with much narrower
    corpus coverage than the rest can still get its own comparable panel without duplicating the
    chart-building logic (see git history for the two-panel version this replaced, from when
    GumTree covered only 5 of 93 fixtures). `legend_labels` overrides the legend text only (e.g.
    to add a per-series "(n=...)") without affecting which column each series reads from `labels`."""
    ax.set_facecolor(SURFACE)
    series = [agreement(panel_rows, label) for label in labels]
    bins = np.arange(0, 101, 10)

    counts, _, bar_containers = ax.hist(
        series, bins=bins, label=legend_labels or labels, color=colors, edgecolor=SURFACE, linewidth=1, zorder=3,
    )
    label_offset = max(np.max(counts), 1) * 0.02
    for container in bar_containers:
        for bar in container:
            if bar.get_height() > 0:
                ax.text(
                    bar.get_x() + bar.get_width() / 2, bar.get_height() + label_offset,
                    f"{int(bar.get_height())}", ha="center", va="bottom", fontsize=8.5, color=INK_SECONDARY, zorder=4,
                )

    ax.set_xlim(0, 100)
    ax.set_ylim(0, max(np.max(counts), 1) * 1.18)
    ax.set_xticks(bins)
    ax.xaxis.set_major_formatter(ticker.FuncFormatter(lambda v, _: f"{v:.0f}%"))
    ax.set_xlabel("Agreement with the human mapping", fontsize=10.5, color=INK_SECONDARY)
    ax.set_ylabel("Number of fixtures", fontsize=10.5, color=INK_SECONDARY)
    ax.yaxis.set_major_locator(ticker.MaxNLocator(integer=True))
    ax.set_title(title, fontsize=11.5, color=INK_PRIMARY, loc="left", pad=10)
    ax.tick_params(colors=INK_MUTED, labelsize=9)
    ax.grid(axis="y", color=GRIDLINE, linewidth=1, zorder=0)
    for spine in ("top", "right"):
        ax.spines[spine].set_visible(False)
    ax.spines["left"].set_color(BASELINE)
    ax.spines["bottom"].set_color(BASELINE)
    ax.legend(loc="upper left", frameon=False, fontsize=9.5, labelcolor=INK_SECONDARY)


def plot_accuracy(rows: list[dict], tools: list[str], output_path: Path) -> None:
    """Grouped histogram, all tools in one panel: how many fixtures fall into each 10-point bucket
    of *agreement* rate (100 - mismatch rate, so higher is better and 100% means the tool matched
    the human mapping on every line).

    Each series is built from that tool's own `applicable_rows` - a fixture outside a tool's
    language scope (`ExternalTool::supports`, e.g. GumTree has no generator for this corpus's one
    JSON fixture) is dropped from that tool's series entirely, not coerced to a 0%-agreement
    fixture it was never scored on. One combined panel is honest here specifically because every
    tool's scope is close to the full corpus (within a fixture or two) - if a future tool covers
    only a small slice, give it a second, separately-scaled panel instead of folding it into these
    bars (see git history around 2026-07 for the two-panel version this replaced, back when GumTree
    covered only 5 of 93 fixtures).

    `treesitter_parse` never appears here, unlike in `plot_runtime` - it produces no line labels,
    so it has nothing to agree or disagree with the human mapping about; it's a runtime reference
    only.
    """
    labels = ["codediff"] + tools
    colors = [CODEDIFF_COLOR] + [TOOL_COLORS[t] for t in tools]
    legend_labels = [f"{label} (n={len(applicable_rows(rows, label))})" for label in labels]

    fig, ax = plt.subplots(figsize=(11, 6.5), facecolor=SURFACE)
    _plot_agreement_histogram(
        ax, rows, labels, colors,
        f"Line-level agreement rate, bucketed in 10-point increments ({len(rows)} fixtures in the corpus)",
        legend_labels=legend_labels,
    )

    output_path.parent.mkdir(parents=True, exist_ok=True)
    fig.savefig(output_path, dpi=150, bbox_inches="tight", facecolor=SURFACE)
    plt.close(fig)
    print(f"Plot saved to {output_path}")


def plot_runtime(rows: list[dict], tools: list[str], output_path: Path) -> None:
    """One violin per tool, plus a `treesitter_parse` reference violin and (when present in the
    CSV) a `gumtree_warm` reference violin, full per-fixture distribution rather than just the mean
    - codediff's 93 per-fixture times span ~3 orders of magnitude (3ms to 3.9s, dominated by file
    size), so a single bar hides exactly the shape that matters here. `violinplot` runs its KDE in
    log10-space (matplotlib's own kernel bandwidth assumes a linear axis, so feeding it raw ms on a
    log-scaled axis would misshape the KDE) - the y-axis ticks are then relabeled back to real ms.

    `treesitter_parse` goes first, ahead of codediff: it's not a competing tool (no accuracy to
    score, see `plot_accuracy`'s doc comment for why it's absent there), it's the reference lower
    bound every AST-aware series to its right must pay before their own work even starts - reading
    left to right is reading the cost stack from the ground up.

    `gumtree_warm` goes last, right after `gumtree`: same algorithm, same accuracy, timed inside a
    persistent JVM instead of a fresh subprocess per fixture (`research/drivers/gumtree-batch/`) -
    it isolates GumTree's own cost from the JVM-startup overhead `gumtree`'s own violin includes.
    `benchmark_other.rs` only fills this column in when the batch driver was actually available for
    that run (see `gumtree_warm_batch`'s doc comment) - every cell blank means the whole column is
    absent, not scored per-fixture like `gumtree_ms` can be, so `tools` (built from `_mismatches`
    columns) never lists it and it needs its own opt-in check here.

    Each tool's violin is built from its own `applicable_rows` (see that function's doc comment) -
    a language-scoped tool like GumTree has far fewer points than codediff/unix_diff, so its violin
    is necessarily noisier and its x-tick carries an explicit "(n=...)" rather than implying the
    same sample size as everything else on the axis. `treesitter_parse_ms` has no scope gaps (every
    corpus language parses), so it's always full sample size, same as codediff."""
    labels = ["treesitter_parse", "codediff"] + tools
    colors = [TREESITTER_COLOR, CODEDIFF_COLOR] + [TOOL_COLORS[t] for t in tools]
    sample_sizes = [len(rows), len(rows)] + [len(applicable_rows(rows, tool)) for tool in tools]
    series_ms = [
        np.array([float(r["treesitter_parse_ms"]) for r in rows]),
        np.array([float(r["codediff_ms"]) for r in rows]),
    ]
    series_ms += [np.array([float(r[f"{tool}_ms"]) for r in applicable_rows(rows, tool)]) for tool in tools]

    if rows and "gumtree_warm_ms" in rows[0] and any(r["gumtree_warm_ms"] != "" for r in rows):
        warm_rows = [r for r in rows if r["gumtree_warm_ms"] != ""]
        labels.append("gumtree_warm")
        colors.append(GUMTREE_WARM_COLOR)
        sample_sizes.append(len(warm_rows))
        series_ms.append(np.array([float(r["gumtree_warm_ms"]) for r in warm_rows]))

    series_log = [np.log10(s) for s in series_ms]

    fig, ax = plt.subplots(figsize=(3 + 2 * len(labels), 5.5), facecolor=SURFACE)
    ax.set_facecolor(SURFACE)

    x = np.arange(len(labels))
    rng = np.random.default_rng(0)
    for xi, log_vals in zip(x, series_log):
        # A light jittered strip under the violin - with only 93 points per tool, the KDE shape
        # alone can suggest more density than the raw points actually support.
        jitter = rng.uniform(-0.07, 0.07, size=len(log_vals))
        ax.scatter(
            np.full(len(log_vals), xi) + jitter, log_vals, s=10, alpha=0.35,
            color=INK_MUTED, linewidth=0, zorder=2,
        )

    parts = ax.violinplot(series_log, positions=x, widths=0.6, showmedians=False, showextrema=False)
    for body, color in zip(parts["bodies"], colors):
        body.set_facecolor(color)
        body.set_edgecolor(color)
        body.set_alpha(0.55)
        body.set_zorder(3)

    for xi, log_vals, color in zip(x, series_log, colors):
        median_log = np.median(log_vals)
        ax.hlines(median_log, xi - 0.3, xi + 0.3, color=INK_PRIMARY, linewidth=2, zorder=4)
        # Starts past the line's own right end (xi + 0.3), not at the violin's center, so the
        # label never sits on top of the violin body or the line itself.
        ax.text(
            xi + 0.36, median_log, f"median {10 ** median_log:.2f} ms",
            ha="left", va="center", fontsize=9, color=INK_SECONDARY, zorder=4,
        )

    # Real-ms tick labels on the log10-transformed axis: 1, 3, 10, 30, ... spans the full range
    # (min ~2.3ms, max ~3.9s) in the familiar 1/3 log steps, not raw log10 values.
    tick_ms = [1, 3, 10, 30, 100, 300, 1000, 3000]
    tick_ms = [v for v in tick_ms if np.log10(v) >= min(s.min() for s in series_log) - 0.3
               and np.log10(v) <= max(s.max() for s in series_log) + 0.3]
    ax.set_yticks(np.log10(tick_ms))
    ax.yaxis.set_major_formatter(ticker.FuncFormatter(lambda v, _: f"{10 ** v:,.3g}"))

    ax.set_xticks(x)
    ax.set_xticklabels([f"{label}\n(n={n})" for label, n in zip(labels, sample_sizes)], fontsize=10, color=INK_MUTED)
    # Extra room on the right for the last violin's median label, which sits past its own x
    # position (see the `ax.text` call above) rather than centered on the violin.
    ax.set_xlim(-0.6, len(labels) - 1 + 1.3)
    ax.set_ylabel("Time per fixture (ms, log scale)", fontsize=11, color=INK_SECONDARY)
    ax.set_title(
        "Runtime distribution: time to produce per-line touched/untouched labels (sample size varies by tool, see x-axis)",
        fontsize=13, color=INK_PRIMARY, loc="left", pad=12,
    )
    ax.tick_params(axis="y", colors=INK_MUTED, labelsize=9)
    ax.grid(axis="y", color=GRIDLINE, linewidth=1, zorder=0, which="major")
    for spine in ("top", "right", "left"):
        ax.spines[spine].set_visible(False)
    ax.spines["bottom"].set_color(BASELINE)

    output_path.parent.mkdir(parents=True, exist_ok=True)
    fig.savefig(output_path, dpi=150, bbox_inches="tight", facecolor=SURFACE)
    plt.close(fig)
    print(f"Plot saved to {output_path}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Compare codediff against other diff tools from benchmark_other.csv.")
    parser.add_argument("--csv", default="benchmark_other.csv", help="Path to the benchmark CSV (default: benchmark_other.csv)")
    parser.add_argument("--plots-dir", default="plots", help="Directory for output PNGs (default: plots/)")
    args = parser.parse_args()

    csv_path = Path(args.csv)
    if not csv_path.exists():
        print(f"No such file: {csv_path}")
        print("Run:  cargo run --release --bin benchmark_other -- --csv")
        raise SystemExit(1)

    fieldnames, rows = read_rows(csv_path)
    print(f"Loaded {csv_path}: {len(rows)} fixtures")

    tools = tool_names(fieldnames)
    print(f"External tools found: {', '.join(tools)}")

    plots_dir = Path(args.plots_dir)
    plot_accuracy(rows, tools, plots_dir / "benchmark_other_accuracy.png")
    plot_runtime(rows, tools, plots_dir / "benchmark_other_runtime.png")
