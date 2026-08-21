#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
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
"""
Assembles *every* number in research/papers/introductory-paper into one generated LaTeX macro
file, `plots/variables.tex`, which that paper's `main.tex` `\\input`s exactly once. No number in
the paper's prose, tables, or abstract is a bare literal; each is a `\\newcommand` written here.

Why one file rather than several: a number that appears in both a table and the prose discussing
it (the line-level mismatch rates appear in both, as do the fixture count and node accuracy) used
to be typed twice, so refreshing the data could update one and miss the other. A single macro used
in both places cannot drift.

Two populations of numbers, deliberately kept visibly distinct in the output:

* GENERATED - read back from an on-disk artifact produced by a measurement run. Refreshing these
  is "re-run the producer, re-run this script." Three blocks:

  - the empirical study (repository/file/language counts, size percentiles, bytes-AST
    correlation), which `file_stats.py::write_paper_variables` writes to
    `variables_empirical.tex` from `stats.sqlite`;
  - RQ1 (whole-tree APTED against a 1-second budget), which `apted_only_report.py::
    write_paper_fragment` writes to `variables_rq1.tex` from `data/rq1/`;
  - the tool comparison (per-tool line-level agreement, wall-clock percentiles), which
    `benchmark_other_report.py::write_paper_fragment` writes to `variables_comparison.tex` from
    `data/comparison/`. Promoted from AUTHORED on 2026-08-20: at ~30 hand-transcribed numbers it
    was the largest authored group, and the one a data refresh touches every single time.

* AUTHORED - a number with no saved producer to read back from, each block below carrying a
  comment naming the command that produced it. These are transcribed, and transcription is exactly
  the failure mode this whole mechanism exists to prevent (see `write_paper_variables`'s own doc
  comment for the slide-deck story). They live here, in one version-controlled place with their
  provenance attached, rather than scattered through `main.tex` with none - a real improvement,
  but not the same guarantee the generated blocks have. What remains authored is the corpus/
  node-accuracy totals, the ablation deltas, the robustness run's status histogram, and the design
  targets; the first three are each one command away from their artifact, recorded per block.

Every macro is emitted on every run, whether or not its source was available. A missing source
emits a loud `\\textbf{??}` placeholder rather than omitting the macro: `main.tex` builds under
`-interaction=nonstopmode`, where an *undefined* macro does not fail the build - it yields a PDF
with the number silently missing, which is strictly worse than the literal it replaced.

Usage (from research/):  uv run ./analysis/paper_variables.py
"""

import os
import re
import sys

PLACEHOLDER = r"\textbf{??}"


def latex_number(value):
    """Formats an int with this project's papers' LaTeX-safe thousands separator - 1234567 ->
    "1{,}234{,}567". A plain comma can trigger LaTeX's comma-in-math spacing rules even in text
    mode. Mirrors `file_stats.py::_latex_number`, kept in sync by
    `test_latex_number_matches_file_stats` below."""
    return f"{value:,}".replace(",", "{,}")


# ---------------------------------------------------------------------------------------------
# AUTHORED values: no saved producer to read these back from. Each block names the command that
# produced it, so a refresh is a known operation rather than an archaeology exercise.
# ---------------------------------------------------------------------------------------------

# Ground-truth corpus size and AST-node accuracy.
# source: cargo run --release --features test-fixtures --bin benchmark_optimal_solutions -- --csv
#         (research/data/quality/optimal_solutions_benchmark.csv), totalled over solved fixtures.
# Measured 2026-08-20. The corpus directory holds 469 fixtures; 468 of them carry a
# human_mapping.json and are what every accuracy number in the paper is scored against. The 469th
# (rust-completely-unrelated-main-files) is deliberately ground-truth-free - it exists as a
# pathological-latency case, not an accuracy case - and reports `human_unsolved` in the CSV.
#
# NumFixtures is therefore the ground-truth-bearing count, 468, which is the denominator of every
# per-tool row, the ablation study, and the node accuracy below.
CORPUS = {
    "NumFixtures": 468,
    "NodesMatched": 4_956_544,
    "NodesTotal": 4_959_531,
    # Distinct languages across the fixture corpus, from `analyze_human_mappings`' own "By
    # language" census (24 as of 2026-08-20). Not the same number as the empirical study's
    # \NumLanguages, which counts languages in the 100-repository file-stats corpus.
    "NumFixtureLanguages": 24,
}

# Same corpus, same run, counting only nodes that carry text of their own and therefore reach the
# screen when the diff is rendered (`codediff::diff::nodes::is_structurally_visible`). Reported
# alongside the all-node figure because the all-node denominator includes every ancestor of every
# change up to the root, so it partly measures how deep a grammar's tree is.
CORPUS_VISIBLE = {
    "VisibleNodesMatched": 3_378_653,
    "VisibleNodesTotal": 3_380_690,
}

# Leave-one-out ablation deltas, in mismatches, against an all-enabled baseline. A positive number
# means disabling the pass HURT accuracy, i.e. the pass earns its place.
# source: `make ablation-study` from research/ (measure/ablation_study.sh), which writes one CSV
#         per run to research/data/ablation/. The script prints the node-granularity table; both
#         granularities below are totalled from those CSVs' `mismatches` and `visible_mismatches`
#         columns, so unlike the pre-2026-08-20 values these are recomputable from artifacts.
#
# Measured 2026-08-20 against the 468-fixture ground-truth corpus. Every one of these four passes
# is a *different* pass from the four the paper's table carried before this refresh: the earlier
# set (import-node normalization, flow-control arm matching, bottom-up expansion, move-detection
# recovery) was measured 2026-07-15, and three of those four have since been deleted from the
# codebase outright. Do not compare the two tables row by row - only move-detection recovery is
# the same pass in both.
#
# The visible-node column is the newer, stricter reading: it counts only nodes that carry text of
# their own and therefore reach the screen. Two passes that measurably help at full node
# granularity move it by exactly zero, i.e. everything they fix is structural interior nodes a
# reader never sees.
ABLATION = {
    "AblationMovedSubtrees": "+2{,}266",
    "AblationBottomUpPropagation": "+318",
    "AblationMutualAncestors": "+23",
    "AblationUniqueTypeMatching": "+0",
}

# Same four passes, same runs, scored on visible nodes only. See ABLATION's comment.
ABLATION_VISIBLE = {
    "AblationVisibleMovedSubtrees": "+1{,}564",
    "AblationVisibleBottomUpPropagation": "+0",
    "AblationVisibleMutualAncestors": "+0",
    "AblationVisibleUniqueTypeMatching": "+0",
}

# NOTE: the per-tool COMPARISON and SPEED blocks that used to live here were promoted to GENERATED
# on 2026-08-20 - `benchmark_other_report.py::write_paper_fragment` now writes them to
# `plots/variables_comparison.tex` from `benchmark_accuracy.csv` (accuracy) and
# `benchmark_other.csv` (timing), and they are merged in below exactly like the empirical and RQ1
# blocks. That removes ~30 hand-transcribed numbers, which were the largest remaining AUTHORED
# group and the one most likely to drift: they are the numbers a refresh touches every time.

# Sampled robustness run over real Rust (repository, commit, file) pairs.
# source: cargo run --release --bin benchmark_diff_pairs --csv data/samples/sampled_code_pairs_rust.csv
#         --repo-root /var/tmp/research/small/repositories/ --output data/performance/robustness_rust.csv
#         Measured 2026-08-20: ok=406, skipped_too_large=142, timed_out=0, panicked=0,
#         failed_to_read=377, over the 925 pairs in sampled_code_pairs_rust.csv.
#
# Recomputing the split from disk: `skipped_too_large` and `ok` are the output CSV's own `status`
# column, and RobustnessSampled is that input sample's row count. `failed_to_read` is the one
# value NOT in the output - a pair whose blob cannot be read never gets a row - so it is the
# difference between the two files' row counts (925 - 548 = 377), not a value to read directly.
# Every one of those 377 is a `revspec ... not found`: these repositories' histories were rewritten
# after the sample was drawn, which is a property of the corpus, not a CodeDiff failure.
ROBUSTNESS = {
    "RobustnessSampled": 925,
    "RobustnessNodeCap": 16_000,
    "RobustnessTimeoutSeconds": 120,
    "RobustnessUnavailable": 377,
    "RobustnessSkipped": 142,
}

# Design targets and fixed descriptive facts. Chosen, not measured - a refresh means a decision,
# not a re-run - except GumTreeVersion, which is whichever build benchmark_other was run against.
TARGETS = {
    "SpeedTargetMs": "400",
    "SpeedTargetPct": "99.99",
    # Clone depth the corpus under /var/tmp/research/full/ was fetched at, per commit from each
    # branch tip (`make fetch MODE=full DEPTH=50`, 2026-08-20). Not a measurement - a parameter of
    # how the corpus was built - but it belongs in the paper: it bounds how far back RQ1's commit
    # sampling can reach. The paper previously claimed the repositories were cloned in full, which
    # was never true of these checkouts.
    "CorpusCloneDepth": "50",
    # Was "seven" until 2026-08-20. The pipeline's phases are numbered 1-7 in the source, but two
    # of those numbers are now vacant: the Dice-coefficient bottom-up expansion that occupied
    # phases 3 and 5 was deleted from the codebase on 2026-08-16 after measuring net-negative.
    # The paper renumbers the five that remain as 1-5 rather than exposing the source's historical
    # gaps, so this word and the paper's phase headings must be changed together.
    "NumPhasesWord": "five",
    "GumTreeVersion": "v4.0.0-beta8",
}


def read_newcommands(path, only=None):
    """Returns the `\\newcommand` lines of a generated .tex file, or None if it doesn't exist or
    defines none. Comment lines are dropped - this script supplies its own header. `only`
    restricts the result to a set of macro names, for pulling one block back out of a file that
    holds several."""
    if not os.path.exists(path):
        return None
    lines = []
    with open(path) as f:
        for line in f:
            line = line.rstrip("\n")
            match = re.match(r"\\newcommand\{\\(\w+)\}", line)
            if match and (only is None or match.group(1) in only):
                lines.append(line)
    return lines or None


def read_generated_block(fragment_path, previous_output_path, expected_macros, refresh_hint):
    """A generated macro block, from the freshest source available.

    Prefers the producer's own fragment, which is what a just-finished measurement run wrote.
    Falls back to this script's own previous output, re-reading that block's macros out of the
    `variables.tex` already on disk.

    That fallback matters because the producers are the slow steps (file-stats is hours over the
    full corpus; rq1 is a multi-hour timing run) and are deliberately *not* prerequisites of the
    paper targets: `make introductory-paper` rebuilds the PDF from whatever is already on disk.
    Without the fallback, any paper rebuild that did not just re-run the producer would replace
    good numbers with placeholders. Reading happens strictly before writing, so re-reading the
    file this script is about to overwrite is safe."""
    fragment = read_newcommands(fragment_path)
    if fragment is not None:
        return fragment, f"{os.path.basename(fragment_path)}"

    previous = read_newcommands(previous_output_path, only=set(expected_macros))
    if previous is not None:
        print(
            f"note: no {os.path.basename(fragment_path)}; carrying that block forward from "
            f"{previous_output_path}. Run `{refresh_hint}` to refresh it.",
            file=sys.stderr,
        )
        return previous, "previous variables.tex"

    return None, "nothing"


# Every macro the empirical fragment is expected to define. Listed explicitly so a missing or
# truncated fragment still produces a complete, if loudly-placeholdered, variables.tex rather
# than a file that silently omits macros main.tex goes on to use.
EMPIRICAL_MACROS = [
    "NumRepos",
    "NumFiles",
    "NumFilesMillions",
    "NumLanguages",
    "CorrelationR",
] + [
    f"{prefix}{suffix}"
    for prefix in ("Bytes", "Loc", "Ast")
    for suffix in ("PFifty", "PNinety", "PNinetyNine", "PNineNineNine", "Max")
]

# Every macro the RQ1 fragment (apted_only_report.py's write_paper_fragment) is expected to
# define. RqOneScriptingPairs/Pct are deliberately NOT listed: the pre-2026-08-18 measurement
# corpus contains no scripting-category languages, so the fragment legitimately omits them until
# the re-measurement against the re-sampled corpus lands - the paper's prose must not cite them
# before then either.
RQ_ONE_MACROS = [
    "RqOnePairsAttempted",
    "RqOneCodePairs",
    "RqOneCodePct",
    "RqOneConfigDataPairs",
    "RqOneConfigDataPct",
    "RqOneCodeTenToThirtyPct",
    "RqOneCodeHundredToThreeHundredPct",
]

# Every macro the comparison fragment (benchmark_other_report.py's write_paper_fragment) is
# expected to define: three accuracy macros for each of the five tools scored at the line level,
# and three wall-clock percentiles for each of the six timing series. GumTree appears in both
# halves under different stems - `GumTree*` for its accuracy row, `SpeedGumTreeCold*`/
# `SpeedGumTreeWarm*` for its two timing series - which is why this list is written out per half
# rather than as one product over a single tool list.
COMPARISON_MACROS = [
    f"{tool}{suffix}"
    for tool in ("CodeDiff", "UnixDiff", "GumTree", "Difftastic", "Diffsitter")
    for suffix in ("Fixtures", "LineMismatches", "LineRate")
] + ["CommonFixtures"] + [
    f"Common{tool}LineRate"
    for tool in ("CodeDiff", "UnixDiff", "GumTree", "Difftastic", "Diffsitter")
] + [
    f"Speed{tool}{suffix}"
    for tool in ("CodeDiff", "UnixDiff", "GumTreeCold", "GumTreeWarm", "Difftastic", "Diffsitter")
    for suffix in ("PFifty", "PNinety", "PNinetyNine")
]

# Every macro the change-shape fragment (human_mapping_shapes_report.py's write_paper_fragment) is
# expected to define: the corpus-wide fixture count and solve rate, plus prevalence and solve rate
# for each shape RQ3 asks about. Shape stems mirror that script's own SHAPES list.
SHAPE_MACROS = ["ShapeFixtures", "ShapeAllSolvedPct", "ShapeAnyReparentPct"] + [
    f"Shape{stem}{suffix}"
    for stem in ("Reparent", "DeepReparent", "Reorder", "MultiMap", "NoShape")
    for suffix in ("Fixtures", "Pct", "SolvedPct")
] + [
    f"Shape{stem}{suffix}"
    for stem in ("AnyReparent", "NoShape")
    for suffix in ("ErrorPct", "FailingPct")
]


def command(name, value):
    return f"\\newcommand{{\\{name}}}{{{value}}}"


def emit_block(fragment_lines, expected_macros, label, refresh_hint):
    """The lines for one generated block: the fragment's own `\\newcommand`s, plus a loud
    `\\textbf{??}` placeholder for every expected macro the fragment did not define.

    Never silently omits a macro, for the reason in this module's doc comment: `main.tex` builds
    under `-interaction=nonstopmode`, where an *undefined* macro yields a PDF with the number
    quietly missing instead of failing the build. A placeholder is visible on the page; an omission
    is not."""
    if fragment_lines is None:
        print(
            f"WARNING: no {label} fragment found from any source - emitting placeholders for "
            f"{len(expected_macros)} macros. Run `{refresh_hint}` first.",
            file=sys.stderr,
        )
        return [command(name, PLACEHOLDER) for name in expected_macros]

    defined = {
        match.group(1)
        for line in fragment_lines
        if (match := re.match(r"\\newcommand\{\\(\w+)\}", line))
    }
    missing = [name for name in expected_macros if name not in defined]
    if missing:
        print(
            f"WARNING: {label} fragment is missing {len(missing)} expected macro(s): "
            f"{', '.join(missing)} - emitting placeholders.",
            file=sys.stderr,
        )
    return fragment_lines + [command(name, PLACEHOLDER) for name in missing]


def build(empirical_lines, rq1_lines, comparison_lines, shape_lines):
    """Returns the complete variables.tex as a list of lines."""
    out = [
        "% Auto-generated by research/analysis/paper_variables.py. Do not edit by hand.",
        "% Regenerate: (cd research && make paper-variables), or via `make introductory-paper` /",
        "% `make introductory-paper-empirical` in research/, which run it for you.",
        "% papers/introductory-paper/figures/variables.tex is a symlink to this file.",
        "%",
        "% Every number in that paper is a macro defined here - see this script's module doc",
        "% comment for which of these blocks are read back from a measurement artifact and which",
        "% are transcribed with their provenance recorded alongside them.",
        "",
        "% --- Empirical study: corpus size, per-file size percentiles, bytes-AST correlation.",
        "% Generated by analysis/file_stats.py from stats.sqlite; refresh with",
        "% `make file-stats MODE=<tiny|small|full>` then re-run this script.",
    ]

    out += emit_block(
        empirical_lines, EMPIRICAL_MACROS, "empirical", "make file-stats MODE=<mode>",
    )

    out += [
        "",
        "% --- RQ1: whole-tree APTED against a 1-second budget, by artifact category.",
        "% Generated by analysis/apted_only_report.py from data/rq1/; refresh with",
        "% `make rq1-report` (fast, existing data) or `make rq1` (full re-measurement).",
    ]
    out += emit_block(rq1_lines, RQ_ONE_MACROS, "RQ1", "make rq1-report")

    out += [
        "",
        "% --- Per-tool line-level agreement and wall-clock percentiles. Generated by",
        "% analysis/benchmark_other_report.py from data/comparison/benchmark_accuracy.csv",
        "% (accuracy) and benchmark_other.csv (timing); refresh with `make benchmark-timing-report`",
        "% (fast, existing data), or `make benchmark-accuracy` / `make benchmark-timing` to",
        "% re-measure. Rates carry no percent sign; the paper adds \\%.",
    ]
    out += emit_block(
        comparison_lines, COMPARISON_MACROS, "comparison", "make benchmark-timing-report",
    )

    out += [
        "",
        "% --- Change-shape census: how often each shape RQ3 asks about occurs in the ground-truth",
        "% corpus, and how often CodeDiff maps a fixture containing it with zero mismatches.",
        "% Generated by analysis/human_mapping_shapes_report.py from",
        "% data/quality/human_mapping_analysis.csv; refresh with `make shapes-report`.",
    ]
    out += emit_block(shape_lines, SHAPE_MACROS, "change-shape", "make shapes-report")

    # Corpus size and node accuracy. NodeMismatches and NodeAccuracyPct are derived here rather
    # than transcribed separately: three independent literals for one measurement can drift apart,
    # and the paper states all three.
    matched = CORPUS["NodesMatched"]
    total = CORPUS["NodesTotal"]
    visible_matched = CORPUS_VISIBLE["VisibleNodesMatched"]
    visible_total = CORPUS_VISIBLE["VisibleNodesTotal"]
    out += [
        "",
        "% --- Ground-truth corpus and AST-node accuracy (AUTHORED - see script's CORPUS block).",
        command("NumFixtures", CORPUS["NumFixtures"]),
        command("NumFixtureLanguages", CORPUS["NumFixtureLanguages"]),
        command("NodesMatched", latex_number(matched)),
        command("NodesTotal", latex_number(total)),
        command("NodeMismatches", latex_number(total - matched)),
        command("NodeAccuracyPct", f"{matched / total * 100:.2f}"),
        command("VisibleNodesMatched", latex_number(visible_matched)),
        command("VisibleNodesTotal", latex_number(visible_total)),
        command("VisibleNodeMismatches", latex_number(visible_total - visible_matched)),
        command("VisibleNodeAccuracyPct", f"{visible_matched / visible_total * 100:.2f}"),
        "",
        "% --- Ablation deltas, in mismatches (AUTHORED - see script's ABLATION block). Signed;",
        "% used inside math mode in the paper's table. Positive = disabling the pass hurt accuracy.",
    ]
    out += [command(name, value) for name, value in ABLATION.items()]
    out += [command(name, value) for name, value in ABLATION_VISIBLE.items()]

    # Completed = sampled - unavailable - skipped, derived for the same reason as NodeMismatches.
    completed = (
        ROBUSTNESS["RobustnessSampled"]
        - ROBUSTNESS["RobustnessUnavailable"]
        - ROBUSTNESS["RobustnessSkipped"]
    )
    out += [
        "",
        "% --- Sampled robustness run (AUTHORED - see script's ROBUSTNESS block).",
    ]
    out += [command(name, latex_number(value)) for name, value in ROBUSTNESS.items()]
    out += [command("RobustnessCompleted", latex_number(completed))]

    out += [
        "",
        "% --- Design targets and fixed descriptive facts (AUTHORED - see script's TARGETS block).",
    ]
    out += [command(name, value) for name, value in TARGETS.items()]

    return out


def main():
    here = os.path.dirname(os.path.abspath(__file__))
    research_dir = os.path.dirname(here)
    plots = os.path.join(research_dir, "plots")
    # plots/ is the single source of truth: papers/introductory-paper/figures/variables.tex is a
    # symlink to this file, so there is exactly one copy and no copy step to keep in sync.
    output_path = os.path.join(plots, "variables.tex")

    empirical, empirical_source = read_generated_block(
        os.path.join(plots, "variables_empirical.tex"), output_path, EMPIRICAL_MACROS,
        "make file-stats MODE=<mode>",
    )
    rq1, rq1_source = read_generated_block(
        os.path.join(plots, "variables_rq1.tex"), output_path, RQ_ONE_MACROS, "make rq1-report",
    )
    comparison, comparison_source = read_generated_block(
        os.path.join(plots, "variables_comparison.tex"), output_path, COMPARISON_MACROS,
        "make benchmark-timing-report",
    )
    shapes, shapes_source = read_generated_block(
        os.path.join(plots, "variables_shapes.tex"), output_path, SHAPE_MACROS,
        "make shapes-report",
    )
    lines = build(empirical, rq1, comparison, shapes)

    os.makedirs(os.path.dirname(output_path), exist_ok=True)
    with open(output_path, "w") as f:
        f.write("\n".join(lines) + "\n")

    macro_count = sum(1 for line in lines if line.startswith("\\newcommand"))
    print(
        f"{macro_count} paper variables written to {output_path} "
        f"(empirical block from: {empirical_source}; RQ1 block from: {rq1_source}; "
        f"comparison block from: {comparison_source}; shape block from: {shapes_source})"
    )


if __name__ == "__main__":
    main()
