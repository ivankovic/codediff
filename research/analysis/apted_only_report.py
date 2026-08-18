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
ast_nodes_after, status, elapsed_ms), buckets every pair by this project's shared LOC buckets
(`stats::sampling::LOC_BUCKETS`, keyed by the larger of before/after LOC - mirrored in
`LOC_BUCKETS` below and drift-checked against the Rust source on every run), and reports/plots the
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

Three caveats the numbers below carry, all worth reading before citing a headline percentage:

1. **The corpus is a size-stratified sample, not a uniform one.** `sample_code_pairs.rs` samples
   per language *per size bucket* specifically so that
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

   The corpus is drawn *and* reported under the same LOC strata as of the 2026-08-18 re-sample, so
   per-bucket counts are close to even (roughly 1.5k-3.1k per bucket) rather than the 271-1450
   spread left over from re-bucketing the older byte-size sample by LOC. Per-cell `n` is annotated
   on the by-category chart; re-draw with `make sample-pairs-all` if a (bucket, category) cell is
   too thin to carry the claim being made from it.

2. **"Source code" is not one population, so this report never reports it as one.** The corpus
   spans general-purpose programming languages, shell/editor scripting, and config/data/markup
   formats, and they behave differently enough that a single blended percentage is misleading: on
   the pre-re-sample corpus, code sat at 40.6% within 1s against config/data's 57.0%, and in the
   100-300 LOC bucket specifically at 9.5% against 43.2%. A blended headline therefore moves with
   how many YAML or JSON files a given sampling run happened to draw, independently of anything
   APTED does to source code. `per_category_table` splits every bucket three ways for this reason;
   RQ1's answer is the `code` block, and the others are context.

3. **LOC is a proxy, not the true cost driver.** APTED's cost is governed by AST node count and
   tree shape (depth, leaf/internal ratio), not lines of code, and LOC-per-node varies widely by
   language and file kind (a JSON translation list is thousands of near-identical shallow lines; a
   dense C++ header is far fewer lines but a deeper, more irregular tree). Re-bucketing this same
   corpus by combined AST node count instead of LOC reproduces the same shape the LOC
   chart shows (see `node_cross_check`'s printout below) - confirming the shape is a real property
   of which languages/tree shapes populate each size range, not an artifact of using LOC as the
   x-axis. LOC is kept as the x-axis here because it is the quantity practitioners reason about,
   not because it is the more precise predictor.

   This is sharper since adopting the project-wide buckets, which key on `max(before, after)`
   rather than the combined before+after LOC this report used previously. APTED compares *both*
   trees, so its cost tracks combined size more closely than either side alone; the node
   cross-check, which is bucketed on combined node count, is the check that the headline shape
   survives that choice of key.

Usage (from research/):
    uv run ./analysis/apted_only_report.py
    uv run ./analysis/apted_only_report.py --results 'data/rq1/apted_only_group*.csv' \
        --plots-dir plots/
"""

import argparse
import csv
import glob
import re
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

# This project's LOC buckets, mirroring `LOC_BUCKETS` in src/stats/sampling.rs: (exclusive upper
# bound, label), keyed by the *larger* of a pair's before/after line count. One definition used
# everywhere - `sample_code_pairs`, `sample_test_diffs --stratified`, and this report - so a label
# like "30-100" means the same thing in every artifact this project produces.
#
# `verify_matches_rust` below parses the Rust constant and fails this script if the two ever drift.
# They are duplicated rather than shared because the Rust side is a compile-time constant and this
# is a standalone uv script; a drift check is the cheapest honest guard against that duplication.
#
# Replaced an earlier, report-local set of edges ([0, 100, 300, 1k, 3k, 10k, 30k, inf] over
# *combined* before+after LOC) that existed only here and matched nothing else in the project.
LOC_BUCKETS = [
    (10, "0-10"),
    (30, "10-30"),
    (100, "30-100"),
    (300, "100-300"),
    (1_000, "300-1000"),
    (3_000, "1000-3000"),
    (float("inf"), "3000+"),
]

RUST_BUCKETS_PATH = Path(__file__).resolve().parents[2] / "src" / "stats" / "sampling.rs"

# What kind of artifact each sampled language is. RQ1 asks about "real-world source-code changes",
# and this corpus is not uniformly that: general-purpose programming languages, shell/editor
# scripting, and config/data/markup formats have visibly different tree shapes, and lumping them
# into one percentage would let (say) a corpus-wide shift in how many YAML files were sampled move
# a number the paper presents as being about source code.
#
# The split is a judgement call at two edges, recorded here rather than left implicit:
#   * LUA and PHP are grouped as CODE, not SCRIPTING. "Scripting" here means shell/editor
#     automation (a `.sh` or `.vim` file), not "dynamically typed" - by the latter reading Python
#     and Ruby would move too, which is not the distinction being drawn.
#   * CSS and HTML are grouped as CONFIG_DATA, not CODE. Neither has functions, control flow, or
#     name binding; structurally they behave like the markup/data formats (wide, shallow, highly
#     repetitive trees), which is what actually drives APTED's cost here.
CODE = "code"
SCRIPTING = "scripting"
CONFIG_DATA = "config/data"

LANGUAGE_CATEGORY = {
    # General-purpose programming languages.
    "C": CODE, "CPP": CODE, "CSharp": CODE, "Go": CODE, "Java": CODE, "JavaScript": CODE,
    "Kotlin": CODE, "LUA": CODE, "PHP": CODE, "Python": CODE, "Ruby": CODE, "Rust": CODE,
    "Swift": CODE, "TSX": CODE, "TypeScript": CODE,
    # Shell / editor automation.
    "ShellScript": SCRIPTING, "Vimscript": SCRIPTING,
    # Config, data, and markup.
    "CSS": CONFIG_DATA, "HTML": CONFIG_DATA, "JSON": CONFIG_DATA, "XML": CONFIG_DATA,
    "YAML": CONFIG_DATA,
}

CATEGORY_ORDER = [CODE, SCRIPTING, CONFIG_DATA]


def verify_matches_rust(path: Path = RUST_BUCKETS_PATH) -> None:
    """Fails if this file's `LOC_BUCKETS` has drifted from the Rust constant it mirrors.

    Skipped with a warning if the Rust file isn't reachable (this script is occasionally run
    against a copied-out results directory rather than a full checkout) - a missing source is not
    evidence of drift, but silently mirroring a constant nobody re-checks is how the two ends of a
    duplicated definition stop agreeing."""
    if not path.exists():
        print(f"warning: {path} not found; skipping LOC_BUCKETS drift check")
        return

    text = path.read_text()
    match = re.search(r"pub const LOC_BUCKETS[^=]*=\s*&\[(.*?)\];", text, re.DOTALL)
    if not match:
        raise SystemExit(f"could not find LOC_BUCKETS in {path} - has it been renamed?")

    entries = re.findall(r'\(\s*([0-9_]+|usize::MAX)\s*,\s*"([^"]+)"\s*\)', match.group(1))
    rust = [
        (float("inf") if bound == "usize::MAX" else float(bound.replace("_", "")), label)
        for bound, label in entries
    ]
    if rust != LOC_BUCKETS:
        raise SystemExit(
            "LOC_BUCKETS has drifted from src/stats/sampling.rs.\n"
            f"  rust:   {rust}\n"
            f"  python: {LOC_BUCKETS}\n"
            "Update this script to match (they must stay identical - see the comment above)."
        )


def bucket_index(loc: np.ndarray) -> np.ndarray:
    """Bucket index per pair, matching `stats::sampling::loc_bucket`'s rule exactly: the first
    bucket whose exclusive upper bound `loc` is strictly less than."""
    return np.digitize(loc, [upper for upper, _ in LOC_BUCKETS[:-1]], right=False)


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


def describe_quantiles(loc_max: np.ndarray, loc_combined: np.ndarray) -> None:
    qs = [0, 10, 25, 50, 75, 90, 95, 99, 100]
    print("LOC quantiles (all attempted pairs):")
    print(f"  {'':4} {'max(before,after)':>18} {'combined':>12}")
    for q in qs:
        print(
            f"  p{q:<3d} {np.percentile(loc_max, q):>18,.0f} "
            f"{np.percentile(loc_combined, q):>12,.0f}"
        )


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


def categories_of(rows: list[dict]) -> np.ndarray:
    """Each row's artifact category (see `LANGUAGE_CATEGORY`). An unmapped language is a hard
    error, not a silent "other" bucket: a language reaching this report without a deliberate
    category is a corpus change nobody classified, and quietly averaging it into the headline
    would be exactly the drift this split exists to prevent."""
    unknown = sorted({r["language"] for r in rows} - LANGUAGE_CATEGORY.keys())
    if unknown:
        raise SystemExit(
            f"unclassified language(s) in the corpus: {unknown}\n"
            "Add them to LANGUAGE_CATEGORY (code / scripting / config-data) before reporting."
        )
    return np.array([LANGUAGE_CATEGORY[r["language"]] for r in rows])


def per_category_table(rows: list[dict], loc_max: np.ndarray, ok: np.ndarray) -> dict:
    """Prints, and returns, the per-bucket within-1s rate split by artifact category.

    This is the breakdown RQ1 should actually be read from: "source-code changes" is a claim about
    the code category, and a corpus whose config/data share shifts between runs would otherwise
    move the headline number without any change in what APTED does to source code."""
    category = categories_of(rows)
    bucket_idx = bucket_index(loc_max)
    results: dict = {}

    for cat in CATEGORY_ORDER:
        cat_mask = category == cat
        n_cat = int(cat_mask.sum())
        if n_cat == 0:
            continue
        languages = sorted({r["language"] for r, m in zip(rows, cat_mask) if m})
        n_ok_cat = int(ok[cat_mask].sum())
        print(
            f"\n{cat.upper()} - {n_cat:,} pairs, {100.0 * n_ok_cat / n_cat:.1f}% within 1s "
            f"({', '.join(languages)}):"
        )
        per_bucket = []
        for i, (_, label) in enumerate(LOC_BUCKETS):
            mask = cat_mask & (bucket_idx == i)
            n = int(mask.sum())
            n_ok = int(ok[mask].sum())
            pct = 100.0 * n_ok / n if n > 0 else float("nan")
            per_bucket.append((label, n, n_ok, pct))
            shown = "     -" if n == 0 else f"{pct:5.1f}%"
            print(f"  {label:>12} LOC   n={n:<6} ok={n_ok:<6} ({shown} processed within 1s)")
        results[cat] = {"n": n_cat, "pct": 100.0 * n_ok_cat / n_cat, "buckets": per_bucket}

    return results


def plot_by_category(results: dict, output_path: Path, total_n: int) -> None:
    """Grouped bar chart: one bar per (LOC bucket, category). Empty (bucket, category) cells are
    left blank rather than drawn as 0%, so a missing bar reads as "nothing sampled here" instead
    of "everything here timed out"."""
    fig, ax = plt.subplots(figsize=(11, 5.5), facecolor=SURFACE)
    ax.set_facecolor(SURFACE)

    cats = [c for c in CATEGORY_ORDER if c in results]
    colors = {CODE: BAR_COLOR, SCRIPTING: "#e08a3c", CONFIG_DATA: "#6d8a5a"}
    labels = [label for label, _, _, _ in results[cats[0]]["buckets"]]
    x = np.arange(len(labels))
    width = 0.8 / len(cats)

    for j, cat in enumerate(cats):
        offset = (j - (len(cats) - 1) / 2) * width
        heights = [0 if np.isnan(p) else p for _, _, _, p in results[cat]["buckets"]]
        ns = [n for _, n, _, _ in results[cat]["buckets"]]
        bars = ax.bar(
            x + offset, heights, width=width * 0.92, color=colors[cat], edgecolor=SURFACE,
            linewidth=0.8, zorder=3, label=f"{cat} (n={results[cat]['n']:,})",
        )
        for bar, n in zip(bars, ns):
            if n == 0:
                continue
            ax.text(
                bar.get_x() + bar.get_width() / 2, bar.get_height() + 1.2, f"{n}",
                ha="center", va="bottom", fontsize=6.5, color=INK_MUTED, zorder=4,
            )

    ax.set_ylim(0, 108)
    ax.set_xticks(x)
    ax.set_xticklabels(labels, fontsize=9.5, color=INK_PRIMARY)
    ax.yaxis.set_major_formatter(ticker.FuncFormatter(lambda v, _: f"{v:.0f}%"))
    ax.set_ylabel("Pairs processed within 1s", fontsize=10.5, color=INK_PRIMARY)
    ax.set_xlabel("Lines of code (larger of before / after)", fontsize=10.5, color=INK_PRIMARY)
    ax.set_title(
        "RQ1: whole-tree tree-edit distance vs. a 1-second budget, by artifact category\n"
        f"({total_n:,} real-world file changes, size-stratified per language - bars are "
        "within-bucket rates; small numbers are per-cell n)",
        fontsize=10.5, color=INK_PRIMARY,
    )
    ax.legend(frameon=False, fontsize=9, loc="upper right")
    ax.grid(axis="y", color=GRIDLINE, zorder=0)
    for spine in ("top", "right"):
        ax.spines[spine].set_visible(False)
    for spine in ("left", "bottom"):
        ax.spines[spine].set_color(INK_MUTED)

    fig.savefig(output_path, dpi=150, bbox_inches="tight", facecolor=SURFACE)
    print(f"Wrote {output_path}")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--results", default="data/rq1/apted_only_group*.csv", help="glob pattern for apted_only_benchmark output CSVs")
    parser.add_argument("--plots-dir", default="plots", type=Path)
    parser.add_argument("--output-png", default="apted_only_rq1.png")
    parser.add_argument("--output-category-png", default="apted_only_rq1_by_category.png")
    parser.add_argument("--describe-only", action="store_true", help="print loc_combined quantiles and exit, without plotting")
    args = parser.parse_args()

    verify_matches_rust()

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

    # Keyed by the larger of the two sides, matching `stats::sampling::loc_bucket` - not by
    # combined before+after LOC, which is what this report used before adopting the project-wide
    # buckets. Combined size is the closer proxy for APTED's actual cost (which scales with the
    # product of the two tree sizes), so this trades a little cost-fidelity on the x-axis for a
    # bucket label that means the same thing here as everywhere else in the project. The AST-node
    # cross-check below, which is bucketed on combined node count, is what guards the shape of the
    # result against that choice.
    loc_max = np.array(
        [max(int(r["loc_before"]), int(r["loc_after"])) for r in attempted], dtype=float
    )
    loc_combined = np.array([int(r["loc_combined"]) for r in attempted], dtype=float)
    describe_quantiles(loc_max, loc_combined)

    if args.describe_only:
        return

    ok = np.array([r["status"] == "ok" for r in attempted], dtype=bool)

    bucket_idx = bucket_index(loc_max)
    n_buckets = len(LOC_BUCKETS)

    labels, pct_ok, ns = [], [], []
    print("\nBucket results:")
    for i in range(n_buckets):
        mask = bucket_idx == i
        n = int(mask.sum())
        n_ok = int(ok[mask].sum())
        pct = 100.0 * n_ok / n if n > 0 else 0.0
        label = LOC_BUCKETS[i][1]
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

    print("\n" + "=" * 78)
    print("Per-category breakdown (see LANGUAGE_CATEGORY for how each language is classified).")
    print("RQ1 asks about source-code changes: the `code` block is the one that answers it.")
    print("=" * 78)
    category_results = per_category_table(attempted, loc_max, ok)

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
    ax.set_xlabel("Lines of code (larger of before / after)", fontsize=10.5, color=INK_PRIMARY)
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

    plot_by_category(
        category_results, args.plots_dir / args.output_category_png, len(attempted)
    )


if __name__ == "__main__":
    main()
