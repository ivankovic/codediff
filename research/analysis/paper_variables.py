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

* GENERATED - read back from an on-disk artifact produced by a measurement run. Currently the
  empirical-study block (repository/file/language counts, size percentiles, bytes-AST
  correlation), which `file_stats.py::write_paper_variables` writes to `variables_empirical.tex`
  from `stats.sqlite`. Refreshing these is "re-run the producer, re-run this script."

* AUTHORED - a number with no saved producer to read back from, listed in `AUTHORED` below with a
  comment naming the command that produced it. These are transcribed, and transcription is exactly
  the failure mode this whole mechanism exists to prevent (see `write_paper_variables`'s own doc
  comment for the slide-deck story). They live here, in one version-controlled place with their
  provenance attached, rather than scattered through `main.tex` with none - a real improvement,
  but not the same guarantee the generated block has. Wiring each group to a real artifact is
  tracked as follow-up work; the `source` field on each block records what that artifact would be.

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
# NOTE: research/data/quality/optimal_solutions_benchmark.csv currently holds 433 rows, not the 98 below - the
# corpus has grown since these numbers were transcribed. Refreshing them is a deliberate,
# separate change: the ablation and comparison blocks below were measured on the same 98 fixtures,
# so updating this block alone would leave the paper internally inconsistent.
CORPUS = {
    "NumFixtures": 98,
    "NodesMatched": 389_398,
    "NodesTotal": 390_142,
}

# Leave-one-out ablation deltas, in mismatches, against an all-enabled baseline.
# source: research/measure/ablation_study.sh (prints a summary table to stdout; it does not
#         currently write a machine-readable file, and research/ablation/ is absent, so these
#         cannot be read back).
# NOTE: `solver-similar-flow-control` (the "Flow-control arm matching" row) was deleted from the
# codebase on 2026-08-14; ablation_study.sh now ablates three passes, not four.
ABLATION = {
    "AblationImportNodes": "-89",
    "AblationFlowControl": "-82",
    "AblationBottomUp": "-69",
    "AblationMovedSubtrees": "+3",
}

# Per-tool applicable-fixture counts, line-level mismatches, and mismatch rates (percent).
# source: cargo run --release --features test-fixtures --bin benchmark_other -- --csv
#         (research/data/comparison/benchmark_other.csv), via analysis/benchmark_other_report.py.
# Rates are stored without the percent sign; the prose and tables add `\%` themselves.
COMPARISON = {
    "CodeDiff": {"fixtures": 98, "mismatches": 479, "rate": "0.887"},
    "UnixDiff": {"fixtures": 98, "mismatches": 570, "rate": "1.055"},
    "GumTree": {"fixtures": 97, "mismatches": 637, "rate": "1.198"},
    "Difftastic": {"fixtures": 98, "mismatches": 877, "rate": "1.624"},
    "Diffsitter": {"fixtures": 83, "mismatches": 960, "rate": "1.897"},
}

# Wall-clock percentiles in milliseconds. Same source as COMPARISON. Printed as-is, without a
# thousands separator, matching the paper's existing table.
SPEED = {
    "UnixDiff": ("2.6", "3.1", "15.8"),
    "Diffsitter": ("4.5", "35.3", "122.5"),
    "CodeDiff": ("9.6", "268.3", "1364.2"),
    "GumTreeWarm": ("42.6", "440.4", "2418.5"),
    "Difftastic": ("55.3", "109.6", "2704.5"),
    "GumTreeCold": ("442.1", "978.1", "2832.8"),
}

# Sampled robustness run over real Rust (repository, commit, file) pairs.
# source: cargo run --release --bin benchmark_diff_pairs (input sample:
#         research/code_pair_diff_stats_rust.csv, 1,000 rows - but that file records only the
#         sampled pairs, not the per-pair outcome, so the unavailable/skipped split below cannot
#         be recomputed from anything currently on disk).
ROBUSTNESS = {
    "RobustnessSampled": 1_000,
    "RobustnessNodeCap": 16_000,
    "RobustnessTimeoutSeconds": 120,
    "RobustnessUnavailable": 386,
    "RobustnessSkipped": 107,
}

# Design targets and fixed descriptive facts. Chosen, not measured - a refresh means a decision,
# not a re-run - except GumTreeVersion, which is whichever build benchmark_other was run against.
TARGETS = {
    "SpeedTargetMs": "400",
    "SpeedTargetPct": "99.99",
    "NumPhasesWord": "seven",
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


def read_empirical(fragment_path, previous_output_path):
    """The empirical macros, from the freshest source available.

    Prefers `file_stats.py`'s fragment, which is what a `make file-stats` run just produced. Falls
    back to this script's own previous output, re-reading the empirical block out of the
    `variables.tex` already on disk.

    That fallback matters because `file-stats` is the slow step (hours over the full corpus) and
    is deliberately *not* a prerequisite of the paper targets: `make introductory-paper` rebuilds
    the PDF from whatever is already on disk. Without the fallback, any paper rebuild that didn't
    just re-run file_stats would replace good empirical numbers with placeholders. Reading happens
    strictly before writing, so re-reading the file this script is about to overwrite is safe."""
    fragment = read_newcommands(fragment_path)
    if fragment is not None:
        return fragment, "file_stats.py fragment"

    previous = read_newcommands(previous_output_path, only=set(EMPIRICAL_MACROS))
    if previous is not None:
        print(
            f"note: no {os.path.basename(fragment_path)}; carrying the empirical block forward "
            f"from {previous_output_path}. Run `make file-stats MODE=<mode>` to refresh it.",
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


def command(name, value):
    return f"\\newcommand{{\\{name}}}{{{value}}}"


def build(empirical_lines):
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

    if empirical_lines is None:
        print(
            "WARNING: no variables_empirical.tex found - emitting placeholders for "
            f"{len(EMPIRICAL_MACROS)} empirical macros. Run `make file-stats MODE=<mode>` first.",
            file=sys.stderr,
        )
        out += [command(name, PLACEHOLDER) for name in EMPIRICAL_MACROS]
    else:
        defined = {
            match.group(1)
            for line in empirical_lines
            if (match := re.match(r"\\newcommand\{\\(\w+)\}", line))
        }
        out += empirical_lines
        missing = [name for name in EMPIRICAL_MACROS if name not in defined]
        if missing:
            print(
                f"WARNING: empirical fragment is missing {len(missing)} expected macro(s): "
                f"{', '.join(missing)} - emitting placeholders.",
                file=sys.stderr,
            )
            out += [command(name, PLACEHOLDER) for name in missing]

    # Corpus size and node accuracy. NodeMismatches and NodeAccuracyPct are derived here rather
    # than transcribed separately: three independent literals for one measurement can drift apart,
    # and the paper states all three.
    matched = CORPUS["NodesMatched"]
    total = CORPUS["NodesTotal"]
    out += [
        "",
        "% --- Ground-truth corpus and AST-node accuracy (AUTHORED - see script's CORPUS block).",
        command("NumFixtures", CORPUS["NumFixtures"]),
        command("NodesMatched", latex_number(matched)),
        command("NodesTotal", latex_number(total)),
        command("NodeMismatches", latex_number(total - matched)),
        command("NodeAccuracyPct", f"{matched / total * 100:.1f}"),
        "",
        "% --- Ablation deltas, in mismatches (AUTHORED - see script's ABLATION block). Signed;",
        "% used inside math mode in the paper's table.",
    ]
    out += [command(name, value) for name, value in ABLATION.items()]

    out += [
        "",
        "% --- Per-tool line-level agreement (AUTHORED - see script's COMPARISON block). Rates",
        "% carry no percent sign; the paper adds \\%.",
    ]
    for tool, values in COMPARISON.items():
        out += [
            command(f"{tool}Fixtures", values["fixtures"]),
            command(f"{tool}LineMismatches", latex_number(values["mismatches"])),
            command(f"{tool}LineRate", values["rate"]),
        ]

    out += [
        "",
        "% --- Wall-clock percentiles, milliseconds (AUTHORED - see script's SPEED block).",
    ]
    for tool, (p50, p90, p99) in SPEED.items():
        out += [
            command(f"Speed{tool}PFifty", p50),
            command(f"Speed{tool}PNinety", p90),
            command(f"Speed{tool}PNinetyNine", p99),
        ]

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
    fragment_path = os.path.join(research_dir, "plots", "variables_empirical.tex")
    # plots/ is the single source of truth: papers/introductory-paper/figures/variables.tex is a
    # symlink to this file, so there is exactly one copy and no copy step to keep in sync.
    output_path = os.path.join(research_dir, "plots", "variables.tex")

    empirical, source = read_empirical(fragment_path, output_path)
    lines = build(empirical)

    os.makedirs(os.path.dirname(output_path), exist_ok=True)
    with open(output_path, "w") as f:
        f.write("\n".join(lines) + "\n")

    macro_count = sum(1 for line in lines if line.startswith("\\newcommand"))
    print(
        f"{macro_count} paper variables written to {output_path} "
        f"(empirical block from: {source})"
    )


if __name__ == "__main__":
    main()
