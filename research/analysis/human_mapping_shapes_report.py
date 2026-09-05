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
""" "Shape" of each human-authored solution in the corpus: for every fixture, what fraction of its
AST nodes are Identical vs. one of the six change operations (Update, MatchButNotIdentical,
Delete, DeleteWithChildren, Insert, InsertWithChildren), and how do those six break down against
each other.

Reads research/data/quality/human_mapping_analysis.csv (produced by `analyze_human_mappings --csv`, see that
binary's own doc comment), one row per fixture with a `node_op_<operation>` count for each of the
seven `HumanOperation` variants - true AST *node instances*, not mapping *entries*: a single
`DeleteWithChildren`/`InsertWithChildren` entry in human_mapping.json can cover an entire subtree,
so it contributes that subtree's full size here, not 1 (see `node_ops`'s doc comment in
analyze_human_mappings.rs, and its `implicit_identical_nodes` column - large files' ground truth is
often sparse, only explicitly marking changed/relevant nodes and leaving the rest implicitly
Identical, which `analyze_human_mappings` folds into `node_op_identical` before writing this CSV).
Using the entry-count `op_<operation>` columns instead would both misrepresent node-weight (one
`InsertWithChildren` entry for a 200-node function reading the same as one single-token `Insert`)
and, for the 62/417 fixtures with sparse ground truth, wildly overstate how much of the file
changed (a huge file with one 460-entry localized edit would look almost entirely non-Identical).

Identical is 99.1% of all node instances corpus-wide (see `analyze_human_mappings`'s own stdout
report), so a plain 7-way stacked bar per fixture would render as one solid color everywhere -
useless for seeing shape. Instead, per fixture:

  density  = (total node instances - node_op_identical) / total node instances
  fraction_<op> = node_op_<op> / (total node instances * density), for each of the six
                  non-Identical operations - i.e. normalized to that fixture's own non-Identical
                  mass, not to the whole fixture

An earlier version of this chart scaled each bar's *height* to density instead of normalizing, so
"how much changed" and "what kind of change" were both visible in one chart. That turned out to be
unreadable in practice: density is heavily right-skewed (median 1.8% of a fixture's nodes change),
so nearly every bar was too short to show its internal color composition at all - only a handful of
outlier fixtures had visible detail. This version drops "how much changed" from the plot entirely
(still reported in the density printout and the CSV) and normalizes every bar to its own density, so
every fixture's *composition* of change is legible regardless of how much of it actually changed.
Fixtures are still sorted by density, descending, so the left-to-right ordering still carries a
"most-changed to least-changed" reading even though bar height no longer does.

Usage (from research/):
    uv run ./analysis/human_mapping_shapes_report.py
    uv run ./analysis/human_mapping_shapes_report.py --csv human_mapping_analysis.csv --plots-dir plots/
"""

import argparse
from pathlib import Path

import matplotlib.pyplot as plt
import numpy as np
from _common import GRIDLINE, INK_MUTED, INK_PRIMARY, INK_SECONDARY, SURFACE, read_rows
from matplotlib import ticker

# Same chart-chrome tokens as benchmark_other_report.py / apted_only_report.py.

# The six non-Identical `HumanOperation` variants, in stacking order (bottom to top), with a
# display label and color. Insert/delete use GitHub's own green/red diff convention (familiar to
# any reader of this paper); "_with_children" is the more saturated shade of the same hue, since
# it is the same kind of edit at larger granularity, not a different kind. Update (a value/label
# change to an otherwise-matched node) and MatchButNotIdentical (matched, but neither identical nor
# a simple value update - e.g. a moved/reformatted subtree) are neither insertion nor deletion, so
# they get their own hues (blue, amber) rather than borrowing red/green.
OPERATIONS = [
    ("node_op_delete_with_children", "Delete (with children)", "#8c1c13"),
    ("node_op_delete", "Delete", "#d1453d"),
    ("node_op_match_but_not_identical", "Match, not identical", "#c98a1f"),
    ("node_op_update", "Update", "#2a78d6"),
    ("node_op_insert", "Insert", "#4caf6e"),
    ("node_op_insert_with_children", "Insert (with children)", "#1f6e3d"),
]
ALL_OPS = ["node_op_identical"] + [k for k, _, _ in OPERATIONS]


def mapped_rows(csv_path: Path) -> list[dict]:
    """Only the fixtures that have a `human_mapping.json`."""
    return [r for r in read_rows(csv_path) if r["has_mapping"] == "true"]


def _int(row: dict, key: str) -> int:
    return int(row[key] or 0)


# The change shapes RQ3 asks about, as predicates over one fixture's row, paired with the LaTeX
# macro stem each contributes. Deliberately *per fixture*, not per mapping entry: an entry-level
# share needs a denominator ("of all non-Identical entries") that this CSV cannot reconstruct, and
# the question RQ3 actually asks - does this shape need its own heuristic - is about how often a
# change containing the shape appears and how well it is handled, which is a per-fixture property.
#
# Not mutually exclusive: one fixture can contain a reparent and a reorder and an ambiguity, so the
# rows do not sum to the corpus. `NoShape` is the genuine complement of all four.
SHAPES = [
    ("Reparent", "wrap/reparent (depth delta 1)", lambda r: _int(r, "depth_delta_1") > 0),
    (
        "DeepReparent",
        "deeper reparent (depth delta 2+)",
        lambda r: _int(r, "depth_delta_2") + _int(r, "depth_delta_3plus") > 0,
    ),
    ("Reorder", "same-kind sibling reorder", lambda r: _int(r, "reorder_signals") > 0),
    ("MultiMap", "human-confirmed ambiguity", lambda r: _int(r, "group_count") > 0),
    (
        "NoShape",
        "none of the above",
        lambda r: (
            _int(r, "depth_delta_1") == 0
            and _int(r, "depth_delta_2") + _int(r, "depth_delta_3plus") == 0
            and _int(r, "reorder_signals") == 0
            and _int(r, "group_count") == 0
        ),
    ),
]


def write_paper_fragment(rows: list[dict], output_path: Path) -> None:
    """Writes the change-shape census the introductory paper's RQ3 table cites, as LaTeX macros.

    A fragment merged into plots/variables.tex by analysis/paper_variables.py, same contract as
    apted_only_report.py and benchmark_other_report.py. Regenerate with `make shapes-report`.

    Per shape: how many fixtures contain it, and what fraction of those CodeDiff maps with zero
    mismatches. The second number is the point - prevalence alone cannot say whether a shape needs
    a dedicated heuristic, and the ablation alone cannot say which shapes are going unserved.

    `current_mismatches` is the CSV's own record of CodeDiff's result on that fixture, so this file
    and data/quality/optimal_solutions_benchmark.csv must come from the same corpus state. Both are
    written by their producers against src/test/data/diffs/, so re-run both after adding fixtures.
    """
    lines = [
        "% Auto-generated by research/analysis/human_mapping_shapes_report.py. Do not edit by",
        "% hand - regenerate: make shapes-report (from research/). Merged into plots/variables.tex",
        "% by analysis/paper_variables.py; see that script's module doc comment.",
        f"\\newcommand{{\\ShapeFixtures}}{{{len(rows)}}}",
    ]
    solved_all = sum(1 for r in rows if _int(r, "current_mismatches") == 0)
    lines.append(f"\\newcommand{{\\ShapeAllSolvedPct}}{{{100.0 * solved_all / len(rows):.1f}}}")

    for stem, _label, predicate in SHAPES:
        selected = [r for r in rows if predicate(r)]
        if not selected:
            continue
        solved = sum(1 for r in selected if _int(r, "current_mismatches") == 0)
        lines += [
            f"\\newcommand{{\\Shape{stem}Fixtures}}{{{len(selected)}}}",
            f"\\newcommand{{\\Shape{stem}Pct}}{{{100.0 * len(selected) / len(rows):.1f}}}",
            f"\\newcommand{{\\Shape{stem}SolvedPct}}{{{100.0 * solved / len(selected):.1f}}}",
        ]

    # Share of the corpus's total mismatch mass, not of fixtures. Both readings are needed and they
    # differ: a shape can appear in many failing fixtures while accounting for a smaller share of
    # the mismatches, or the reverse. Reported for reparenting at any depth (the union of the two
    # reparent rows, since depth 1 and depth 2+ are the same phenomenon at different magnitudes)
    # and for the no-shape complement, which is the honest counterweight - a third of the remaining
    # error sits in fixtures exhibiting none of these shapes, so no single shape explains it all.
    total_mismatches = sum(_int(r, "current_mismatches") for r in rows)

    def any_reparent(r):
        return (
            _int(r, "depth_delta_1") + _int(r, "depth_delta_2") + _int(r, "depth_delta_3plus") > 0
        )

    no_shape = next(p for stem, _, p in SHAPES if stem == "NoShape")
    for stem, predicate in (("AnyReparent", any_reparent), ("NoShape", no_shape)):
        mass = sum(_int(r, "current_mismatches") for r in rows if predicate(r))
        failing = [r for r in rows if predicate(r) and _int(r, "current_mismatches") > 0]
        all_failing = sum(1 for r in rows if _int(r, "current_mismatches") > 0)
        lines += [
            f"\\newcommand{{\\Shape{stem}ErrorPct}}{{{100.0 * mass / total_mismatches:.1f}}}",
            f"\\newcommand{{\\Shape{stem}FailingPct}}{{{100.0 * len(failing) / all_failing:.1f}}}",
        ]
    lines.append(
        f"\\newcommand{{\\ShapeAnyReparentPct}}{{"
        f"{100.0 * sum(1 for r in rows if any_reparent(r)) / len(rows):.1f}}}"
    )

    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text("\n".join(lines) + "\n")
    macro_count = sum(1 for line in lines if line.startswith("\\newcommand"))
    print(f"{macro_count} paper shape variables written to {output_path}")

    print(f"\n{'shape':<34}{'fixtures':>10}{'share':>9}{'zero-mismatch':>16}")
    for stem, label, predicate in SHAPES:
        selected = [r for r in rows if predicate(r)]
        if not selected:
            continue
        solved = sum(1 for r in selected if _int(r, "current_mismatches") == 0)
        print(
            f"  {label:<32}{len(selected):>10}{100.0 * len(selected) / len(rows):>8.1f}%"
            f"{100.0 * solved / len(selected):>15.1f}%"
        )
    print(
        f"  {'(whole corpus)':<32}{len(rows):>10}{100.0:>8.1f}%{100.0 * solved_all / len(rows):>15.1f}%"
    )


def main() -> None:
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    parser.add_argument("--csv", default="data/quality/human_mapping_analysis.csv", type=Path)
    parser.add_argument("--plots-dir", default="plots", type=Path)
    parser.add_argument("--output-png", default="human_mapping_shapes.png")
    args = parser.parse_args()

    rows = mapped_rows(args.csv)
    print(f"Loaded {len(rows)} fixtures with a human mapping from {args.csv}")

    write_paper_fragment(rows, args.plots_dir / "variables_shapes.tex")

    totals = np.array([sum(int(r[k]) for k in ALL_OPS) for r in rows], dtype=float)
    nonzero = totals > 0
    if (~nonzero).sum():
        print(f"Dropping {(~nonzero).sum()} fixture(s) with zero AST node instances")
    rows = [r for r, keep in zip(rows, nonzero) if keep]
    totals = totals[nonzero]

    fractions = {
        k: np.array([int(r[k]) for r in rows], dtype=float) / totals for k, _, _ in OPERATIONS
    }
    density = sum(fractions.values())

    order = np.argsort(-density)
    rows = [rows[i] for i in order]
    density_sorted = density[order]
    fractions = {k: v[order] for k, v in fractions.items()}

    print("Density (fraction of a fixture's AST node instances that are non-Identical):")
    for label, q in [("min", 0), ("p25", 25), ("median", 50), ("p75", 75), ("max", 100)]:
        print(f"  {label:<8} {np.percentile(density_sorted, q):.3f}")
    print(f"  mean     {density_sorted.mean():.3f}")

    # Dominance typology: among a fixture's own non-Identical mass (its density, not the whole
    # file), is one single operation doing most of the work, or is it a genuine blend? Answers
    # whether the chart's left-to-right color gradient is a real regime change (e.g. "small edits
    # skew insert-heavy, big edits skew match-not-identical-heavy") or just an artifact of sorting
    # by density with no such structure - print-only, not plotted, since it's a corpus-wide summary
    # rather than a per-fixture number.
    DOMINANCE_THRESHOLD = 0.6
    print(
        f"\nFixtures where one operation is >={DOMINANCE_THRESHOLD:.0%} of the non-Identical mass:"
    )
    nonzero_density = density_sorted > 0
    for key, label, _ in OPERATIONS:
        share_of_change = np.divide(
            fractions[key], density_sorted, out=np.zeros_like(density_sorted), where=nonzero_density
        )
        dominant = (share_of_change >= DOMINANCE_THRESHOLD) & nonzero_density
        print(f"  {label:<26} {dominant.sum():>4}/{nonzero_density.sum()}")
    mixed = nonzero_density.sum() - sum(
        int(
            (
                np.divide(
                    fractions[k],
                    density_sorted,
                    out=np.zeros_like(density_sorted),
                    where=nonzero_density,
                )
                >= DOMINANCE_THRESHOLD
            )[nonzero_density].sum()
        )
        for k, _, _ in OPERATIONS
    )
    print(f"  {'(no single operation dominant)':<26} {mixed:>4}/{nonzero_density.sum()}")

    # Plotted view: each bar normalized to its own non-Identical mass (density), not to the whole
    # fixture - a fixture that's 0.4% changed and one that's 60% changed both fill 0-100%, so the
    # *composition* of change is legible everywhere along the x-axis, not just in the few fixtures
    # with enough density to show up as more than a sliver at the density-scaled height used
    # earlier. This trades away "how much changed" (still available via the density printout/CSV
    # above, and via sort order - fixtures are still ordered by density, so the typology found
    # above still reads left-to-right) for "what kind of change" being readable for every fixture,
    # which is what was asked for. Zero-density fixtures (pure-Identical, no composition to show)
    # are dropped rather than plotted as an empty/undefined bar.
    plot_mask = density_sorted > 0
    if (~plot_mask).sum():
        print(
            f"\nDropping {(~plot_mask).sum()} zero-density fixture(s) (pure-Identical, nothing to compose) from the plot"
        )
    normalized = {
        key: np.divide(vals, density_sorted, out=np.zeros_like(density_sorted), where=plot_mask)[
            plot_mask
        ]
        for key, vals in fractions.items()
    }

    n = int(plot_mask.sum())
    x = np.arange(n)

    fig, ax = plt.subplots(figsize=(13, 6), facecolor=SURFACE)
    ax.set_facecolor(SURFACE)

    bottom = np.zeros(n)
    for key, label, color in OPERATIONS:
        vals = normalized[key]
        ax.bar(x, vals, bottom=bottom, width=1.0, color=color, linewidth=0, label=label, zorder=3)
        bottom += vals

    ax.set_xlim(-0.5, n - 0.5)
    ax.set_ylim(0, 1.0)
    ax.set_xticks([])
    ax.set_xlabel(
        f"Fixtures (n={n}), sorted by density of non-Identical AST nodes",
        fontsize=10.5,
        color=INK_PRIMARY,
    )
    ax.set_ylabel("Share of a fixture's non-Identical AST nodes", fontsize=10.5, color=INK_PRIMARY)
    ax.yaxis.set_major_formatter(ticker.FuncFormatter(lambda v, _: f"{v:.0%}"))
    ax.set_title(
        "Shape of human-authored solutions: composition of change per fixture\n"
        "(each bar = 100% of that fixture's changed nodes; Identical excluded; sorted by density, most-changed left)",
        fontsize=11,
        color=INK_PRIMARY,
    )
    ax.grid(axis="y", color=GRIDLINE, zorder=0)
    for spine in ("top", "right"):
        ax.spines[spine].set_visible(False)
    for spine in ("left", "bottom"):
        ax.spines[spine].set_color(INK_MUTED)
    # Every bar now spans the full 0-100% height (see the normalization above), so there is no
    # empty space left inside the axes to tuck an inset legend into without it sitting on top of
    # bars - placed outside the axes instead (`bbox_inches="tight"` on savefig below expands the
    # saved image to include it, rather than clipping it off).
    ax.legend(
        loc="upper left",
        bbox_to_anchor=(1.01, 1.0),
        frameon=False,
        fontsize=9,
        labelcolor=INK_SECONDARY,
    )

    args.plots_dir.mkdir(parents=True, exist_ok=True)
    output_path = args.plots_dir / args.output_png
    fig.savefig(output_path, dpi=150, bbox_inches="tight", facecolor=SURFACE)
    print(f"\nWrote {output_path}")


if __name__ == "__main__":
    main()
