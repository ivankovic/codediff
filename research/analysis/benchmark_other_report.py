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
"""Compare codediff against other diff tools - Unix `diff`, GumTree, difftastic, diffsitter today
(`benchmark_other.rs`'s `ExternalTool` can grow more) - on line-level agreement with the
human-authored mapping, and on runtime.

Reads benchmark_other.csv (produced by
`cargo run --release --bin benchmark_other -- --csv --repeats N`), one row per fixture with
`<tool>_mismatches` (a single count - deterministic, only ever computed once) and `<tool>_ms`
(every one of the `--repeats` timing measurements for that fixture, `;`-joined into one field -
see `ms_values`) columns for codediff and every `ExternalTool`. See that binary's own doc comment
for what "mismatch" means (a line where the tool's touched/untouched call disagrees with the human
mapping projected down to lines - the only signal a line-based tool like Unix diff can produce at
all, so it's the fairest common ground, not node-level accuracy). A CSV from a pre-`--repeats`
build (or `--repeats 1`) still reads fine - each `_ms` field is just a one-element list then.

Writes three plots:

  - benchmark_other_accuracy.png: grouped histogram of *agreement* rate (100 - mismatch rate, so
    higher is better and 100% means every line matched the human mapping), bucketed into fixed
    10-point-wide bins spanning the full 0-100% axis - one color per tool, sharing the same bins,
    so the two distributions sit side by side within each bucket.
  - benchmark_other_runtime.png: full runtime distribution (every individual repeat, not a
    per-fixture mean), one violin per tool plus a `treesitter_parse_ms` reference violin (log-scale
    y-axis, KDE computed in log10-space - codediff's per-fixture times span ~3 orders of magnitude,
    so a single mean bar hides exactly the shape that matters here). The reference violin is
    tree-sitter parsing alone, with no diffing on top - the lower bound every other series must pay
    before its own work even starts.
Also writes one table (not a plot - a handful of summary numbers per tool reads better as a table
than as a chart):

  - benchmark_other_variance.tex: per-fixture coefficient of variation (stddev/mean across that
    fixture's repeats, as a %), median and p90 per tool - added 2026-07-26 (as a plot) after
    `benchmark_other`'s own aggregate median/p90/max turned out to swing by roughly +-10% between
    back-to-back single-shot runs on a loaded machine, making a single run's numbers untrustworthy
    for judging a real speed change. Changed to a table 2026-07-31. Answers "how much should one
    run's number be trusted" as a companion to the runtime plot's "what does the distribution look
    like." A complete, ready-to-`\\input`-ed ACM `table` environment, so
    `research/papers/introductory-paper/main.tex` can include it directly and it never goes stale
    relative to whatever `benchmark_other.csv` says.

And one LaTeX macro fragment:

  - variables_comparison.tex: the per-tool line-level agreement and wall-clock percentiles the
    introductory paper's comparison and speed tables cite, as `\\newcommand`s. Added 2026-08-20;
    before that these ~30 numbers were AUTHORED entries in `paper_variables.py`, i.e. transcribed
    by hand from a console table, which is the exact failure mode the whole `variables.tex`
    mechanism exists to prevent (see `file_stats.py::write_paper_variables`'s doc comment for the
    slide-deck story). A fragment, not the file `main.tex` reads: `analysis/paper_variables.py`
    merges it into the single `plots/variables.tex`, same contract as
    `apted_only_report.py::write_paper_fragment`.

    The accuracy half reads `data/comparison/benchmark_accuracy.csv` (`make benchmark-accuracy`),
    not this script's own `--csv` - that file carries an explicit per-tool `_status` column, so
    "scored" is a recorded fact rather than inferred from a blank cell, and it is machine- and
    load-independent. The speed half necessarily reads the timing CSV. Passing an accuracy CSV is
    therefore optional: without it, the speed macros are still written and the accuracy ones are
    simply absent, which `paper_variables.py` reports as a missing-macro warning rather than
    silently dropping.

Usage (from research/):
    uv run ./analysis/benchmark_other_report.py
    uv run ./analysis/benchmark_other_report.py --csv benchmark_other.csv --plots-dir plots/
"""

import argparse
import csv
from pathlib import Path

import matplotlib.pyplot as plt
import numpy as np
from matplotlib import ticker

# Chart chrome, from the dataviz skill's reference palette (light mode) - same tokens
# matching_reasons_report.py and diff_pairs_benchmark_comparison.py use.
SURFACE = "#fcfcfb"
INK_PRIMARY = "#0b0b0b"
INK_SECONDARY = "#52514e"
INK_MUTED = "#898781"
GRIDLINE = "#e1e0d9"
BASELINE = "#c3c2b7"

# Display name and color for every series this report can plot, keyed by the internal id
# `benchmark_other.rs`'s CSV columns use (e.g. "gumtree_ms", "gumtree_mismatches"). `DISPLAY_ORDER`
# is this dict's insertion order - the canonical left-to-right/legend order every plot below uses
# (see `ordered`). Not every plot shows every entry: `plot_accuracy` never shows `treesitter_parse`
# or `gumtree_warm`, since neither produces line labels to score for agreement (see that
# function's own doc comment).
#
# `treesitter_parse` (black, INK_PRIMARY) and `unix_diff` (grey, INK_MUTED) reuse this file's own
# chart-chrome tokens rather than a categorical color - a deliberate choice (2026-07-31): neither
# is "a tool being compared" the way codediff/gumtree/difftastic/diffsitter are, one's a reference
# lower bound, the other the long-standing line-level baseline every other series is measured
# against. codediff/gumtree/gumtree_warm's colors (blue/violet/red) are from the dataviz skill's
# reference palette, originally validated colorblind-safe via validate_palette.js as part of a
# larger fivesome that also included unix_diff/treesitter_parse's old categorical slots, before
# those two moved to grey/black above. difftastic/diffsitter (gold/teal) were added 2026-07-31,
# after that validation pass - the dataviz skill and validate_palette.js aren't available in this
# environment, so these two were picked by hand as the two hue families the rest don't already
# cover, mirroring how Okabe-Ito's colorblind-safe 8-palette adds sky-blue and yellow to a base 5
# for the same reason. Re-validate the full current set before treating it as confirmed
# colorblind-safe.
#
# The four git variants (added 2026-08-23) are one engine reached through one flag, so they share a
# single hue family (orange) at four lightnesses rather than taking four unrelated categorical
# slots - the point a reader should take from the chart is "these are the same tool", and four
# scattered hues would say the opposite. BDiff (green) is a genuinely separate text-based tool and
# gets its own slot. Re-validate the full set for colorblind-safety before treating it as
# confirmed; see the note above.
DISPLAY_NAMES = {
    "treesitter_parse": "TreeSitter parse (lower bound)",
    "unix_diff": "UNIX diff (baseline)",
    "git_myers": "git (Myers)",
    "git_minimal": "git (minimal)",
    "git_patience": "git (patience)",
    "git_histogram": "git (histogram)",
    "bdiff": "BDiff (per process)",
    "nvim_diff": "nvim -d",
    "bdiff_warm": "BDiff (warm interpreter)",
    "codediff": "CodeDiff",
    "gumtree": "GumTree (binary)",
    "gumtree_warm": "GumTree (warm JVM)",
    "diffsitter": "diffsitter",
    "difftastic": "difftastic",
}
DISPLAY_ORDER = list(DISPLAY_NAMES)
COLORS = {
    "treesitter_parse": INK_PRIMARY,
    "unix_diff": INK_MUTED,
    "git_myers": "#e07b39",
    "git_minimal": "#f0a875",
    "git_patience": "#b85c1e",
    "git_histogram": "#8a3f10",
    "bdiff": "#3f8f4f",
    "nvim_diff": "#57a773",
    "bdiff_warm": "#7fbf8f",
    "codediff": "#2a78d6",
    "gumtree": "#4a3aa7",
    "gumtree_warm": "#e34948",
    "difftastic": "#c9a227",
    "diffsitter": "#1a9e96",
}

# Which family each comparable tool belongs to. The accuracy chart is split on this: nine series in
# one grouped histogram is unreadable, and text-vs-AST is the split the comparison is actually
# about, not an arbitrary halving to fit the page.
TEXT_TOOLS = [
    "unix_diff",
    "git_myers",
    "git_minimal",
    "git_patience",
    "git_histogram",
    "bdiff",
    "nvim_diff",
]
AST_TOOLS = ["gumtree", "difftastic", "diffsitter"]


def ordered(ids: list[str]) -> list[str]:
    """`ids` sorted into `DISPLAY_ORDER`'s canonical order - lets each plot ask for "whichever of
    these series exist" without hardcoding which subset or order applies to it."""
    return [i for i in DISPLAY_ORDER if i in ids]


def rows_for(id_: str, rows: list[dict]) -> list[dict]:
    """Which of `rows` series `id_` draws from. `treesitter_parse`/`codediff` always draw from the
    full corpus (every language parses, and codediff has no scope gaps of its own). `gumtree_warm`
    draws from whichever rows the batch driver actually covered that run - its own opt-in
    availability check, not a per-fixture language scope (see `plot_runtime`'s doc comment). Every
    other id is `applicable_rows` - `ExternalTool::supports`'s per-fixture language scope."""
    if id_ in ("treesitter_parse", "codediff") or id_ in TEXT_TOOLS:
        # Every text-based tool is language-agnostic (`ExternalTool::supports` returns true for
        # all of them), so like codediff they always draw from the full corpus.
        return rows
    if id_ in ("gumtree_warm", "bdiff_warm"):
        return [r for r in rows if r.get(f"{id_}_ms", "") != ""]
    return applicable_rows(rows, id_)


def has_warm(rows: list[dict], id_: str) -> bool:
    """Whether `<id_>_ms` is present and non-empty for at least one row - the generalization of
    `has_gumtree_warm` below, needed once BDiff gained the same cold/warm split (see
    `bdiff_warm_batch` in benchmark_other.rs)."""
    column = f"{id_}_ms"
    return bool(rows) and column in rows[0] and any(r[column] != "" for r in rows)


def has_gumtree_warm(rows: list[dict]) -> bool:
    """Whether `gumtree_warm_ms` is present and non-empty for at least one row - the batch driver
    is opt-in per *run*, not per-fixture (see `plot_runtime`'s doc comment), so this is checked
    once per report rather than treated like a per-fixture language scope."""
    return (
        bool(rows)
        and "gumtree_warm_ms" in rows[0]
        and any(r["gumtree_warm_ms"] != "" for r in rows)
    )


def read_rows(csv_path: Path) -> tuple[list[str], list[dict]]:
    with open(csv_path, newline="") as f:
        reader = csv.DictReader(f)
        rows = list(reader)
        return reader.fieldnames or [], rows


def tool_names(fieldnames: list[str]) -> list[str]:
    """Every `ExternalTool` present in the CSV, derived from its `<tool>_mismatches` column -
    `benchmark_other.rs`'s `ExternalTool::ALL` is the source of truth for which tools exist, so
    this just mirrors whatever that produced rather than hardcoding "unix_diff"."""
    return [
        c[: -len("_mismatches")]
        for c in fieldnames
        if c.endswith("_mismatches") and c != "codediff_mismatches"
    ]


def pct(mismatches: np.ndarray, total: np.ndarray) -> np.ndarray:
    return np.divide(
        mismatches * 100, total, out=np.zeros_like(mismatches, dtype=float), where=total > 0
    )


def ms_values(row: dict, column: str) -> list[float]:
    """Parses a `benchmark_other.rs` timing column - `write_csv`'s `join_ms` writes every
    `--repeats` measurement for that fixture as a single `;`-joined field (e.g. `"12.3;13.1;12.8"`
    for 3 repeats), not a mean, so the actual per-repeat spread survives into this report rather
    than being collapsed before it's ever plotted. Empty string (tool not applicable to this
    fixture's language, or the field is genuinely absent) returns `[]`, same "not scored" meaning
    `applicable_rows` already uses for the sibling `_mismatches` column."""
    raw = row.get(column, "")
    return [float(v) for v in raw.split(";")] if raw else []


def ms_median(row: dict, column: str) -> float | None:
    """The single representative value for `row[column]`'s repeats, used wherever a plot needs one
    number per fixture rather than the full spread (e.g. the accuracy histogram's bucketing is
    unaffected by timing at all, but a future per-fixture summary would want this) - median, not
    mean, so one slow outlier repeat (a GC pause, a scheduler hiccup) doesn't move the summary as
    much as it would move a mean."""
    values = ms_values(row, column)
    return float(np.median(values)) if values else None


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
    ax,
    panel_rows: list[dict],
    labels: list[str],
    colors: list[str],
    title: str,
    legend_labels: list[str] | None = None,
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
        series,
        bins=bins,
        label=legend_labels or labels,
        color=colors,
        edgecolor=SURFACE,
        linewidth=1,
        zorder=3,
    )
    # Per-bar counts only while they can actually be read. Past four series the bars are narrow
    # enough that adjacent labels overlap into an unreadable smear ("459459459..."), which looks
    # like a rendering bug rather than data - and the text panel, where six near-identical tools
    # stack up, is exactly that case. The summary chart carries the precise numbers instead.
    if len(bar_containers) <= 4:
        label_offset = max(np.max(counts), 1) * 0.02
        for container in bar_containers:
            for bar in container:
                if bar.get_height() > 0:
                    ax.text(
                        bar.get_x() + bar.get_width() / 2,
                        bar.get_height() + label_offset,
                        f"{int(bar.get_height())}",
                        ha="center",
                        va="bottom",
                        fontsize=8.5,
                        color=INK_SECONDARY,
                        zorder=4,
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


# Which granularity each tool's *output* actually carries. This groups the bucket table's rows, and
# it labels a capability, not a measurement: every number in that table is line-level agreement,
# including for the tools listed here as sub-line. See `write_bucket_table`'s caption.
#
# Line-only means the tool's output contains no sub-line information at all - Unix diff and git
# emit hunk headers and whole lines, and nothing finer exists to extract. Sub-line means the tool
# reports character or column ranges inside a line: difftastic's per-change `start`/`end` columns,
# GumTree's and diffsitter's character offsets (both go through `span_from_char_offsets` in
# benchmark_other.rs), BDiff's `str_diff` ranges, and codediff's own `TextRange` columns.
#
# `nvim -d` is in the sub-line group: its line pass is libxdiff, the same engine as the four git
# rows, but it adds a second pass that highlights changed characters *within* those lines, which
# `diff_hlID(lnum, col)` exposes per column. It was briefly excluded on the grounds that its line
# set matched git_myers on 38 of a 40-fixture sample - that was the wrong call. A 2-in-40
# divergence is the same order as git_myers against unix_diff, which get separate rows, and
# "redundant at the granularity we happen to measure" is a statement about the metric, not about
# the tool. It is measured, and the corpus decides.
GRANULARITY = {
    "line": ["unix_diff", "git_myers", "git_minimal", "git_patience", "git_histogram"],
    "subline": ["bdiff", "nvim_diff", "diffsitter", "difftastic", "gumtree", "codediff"],
}

# Agreement buckets, most-accurate first. Checked in order and first match wins, so a fixture at
# exactly 99.0% lands in ">=99%" and one at exactly 95.0% lands in "95-99%".
#
# "Perfect" is zero mismatched lines, not a rounded 100%: a 4,000-line fixture with one bad line
# rounds to 100.0% while not being perfect, and that distinction is the entire point of the column.
BUCKETS = [
    ("Perfect", lambda mismatches, agreement: mismatches == 0),
    (r"$\ge$99\%", lambda mismatches, agreement: agreement >= 99.0),
    (r"95--99\%", lambda mismatches, agreement: agreement >= 95.0),
    (r"$<$95\%", lambda mismatches, agreement: True),
]
PLAIN_BUCKET_LABELS = ["Perfect", ">=99%", "95-99%", "<95%"]


# The metrics a bucket table can be built over: `(mismatch column suffix, denominator column)`.
#
# "line" is the metric every tool here can be scored on, since a line is the only unit a purely
# line-based tool reports at all. "node" is finer, and is only populated for the tools whose output
# carries sub-line structure - `benchmark_other.rs` leaves it blank (status `line_only`) for Unix
# diff and the four git algorithms by construction. That split is the whole reason the node table
# exists: the line table cannot distinguish a tool that marks a whole changed line from one that
# marks the three characters that actually changed, and for BDiff and `nvim -d` that difference is
# the only thing they add over `git diff`.
METRICS = {
    "line": ("line_mismatches", "total_lines"),
    "node": ("node_mismatches", "total_nodes"),
    "visible_node": ("visible_node_mismatches", "total_visible_nodes"),
}


def bucket_counts(accuracy_rows, tool, metric="line"):
    """`(fixtures scored, [count per BUCKETS entry])` for `tool`, or None if it scored nothing.

    Scored on the tool's own applicable subset, exactly like every other number in this report: a
    fixture outside its language scope is excluded, never counted as a failure and never as a
    perfect score. A tool with no score at all for `metric` (a line-only tool asked for its node
    numbers) returns None and simply does not appear in that table, rather than appearing as a
    row of zeros that would read as perfection.
    """
    column, denominator = METRICS[metric]
    scored = [
        r
        for r in accuracy_rows
        if r.get(f"{tool}_status") != "unsupported" and r.get(f"{tool}_{column}", "") != ""
    ]
    if not scored:
        return None
    counts = [0] * len(BUCKETS)
    for row in scored:
        total = int(row[denominator])
        mismatches = int(row[f"{tool}_{column}"])
        agreement_pct = 100.0 * (total - mismatches) / total if total else 100.0
        for index, (_, predicate) in enumerate(BUCKETS):
            if predicate(mismatches, agreement_pct):
                counts[index] += 1
                break
    return len(scored), counts


def write_bucket_table(accuracy_rows, output_path, include_codediff):
    r"""Per-fixture agreement buckets as a generated LaTeX table, ``\input`` directly.

    This replaced the bucketed-agreement histogram (2026-08-23), which could not show the result it
    existed to show: with 10-point buckets every tool put the large majority of its fixtures into
    the single 90--100% bar, so the chart's whole dynamic range sat inside one column. These
    buckets zoom in where the data actually is, and what they expose is not a small difference -
    CodeDiff maps 418 of 486 fixtures with zero mismatched lines against Unix diff's 244, where the
    pooled line rates (0.80% against 1.13%) look nearly tied. Both readings are true: the pooled
    rate is dominated by a handful of very large fixtures, and this one weights every change
    equally.

    Percentages as well as counts, because the rows do not share a denominator - diffsitter scores
    303 fixtures and Unix diff 486, so a bare count of 103 against 244 understates diffsitter badly.
    """
    backslash = "\\"
    row_end = backslash * 2
    header_labels = " & ".join(label for label, _ in BUCKETS)
    lines = [
        "% Auto-generated by research/analysis/benchmark_other_report.py. Do not edit by hand -",
        "% regenerate: make benchmark-timing-report (from research/).",
        # `table*`, not `table`: six columns of "244 (50%)" cells overflow a single ACM
        # column and collide with the neighbouring table (observed 2026-08-23).
        r"\begin{table*}",
        (
            r"  \caption{Per-fixture agreement with the human mapping, bucketed. Every number is"
            r" \emph{line-level} agreement, including for the tools grouped as reporting sub-line"
            r" detail -- that grouping describes what each tool's output carries, not how it was"
            r" scored. ``Perfect'' means zero mismatched lines, not a rounded 100\%. Each tool is"
            r" scored on its own applicable subset ($n$), so percentages, not counts, are"
            r" comparable across rows.}"
        ),
        r"  \label{tab:agreement-buckets}",
        r"  \small",
        r"  \begin{tabular}{lrrrrr}",
        r"    \toprule",
        f"    Tool & $n$ & {header_labels} {row_end}",
        r"    \midrule",
    ]
    group_titles = {
        "line": r"    \multicolumn{6}{l}{\emph{Line granularity only}} " + row_end,
        "subline": r"    \multicolumn{6}{l}{\emph{Reports sub-line detail (not exercised here --"
        r" see Table~\ref{tab:agreement-buckets-node})}} " + row_end,
    }
    for group, members in GRANULARITY.items():
        lines.append(group_titles[group])
        for tool in members:
            if tool == "codediff" and not include_codediff:
                continue
            result = bucket_counts(accuracy_rows, tool)
            if result is None:
                continue
            scored, counts = result
            cells = " & ".join(
                f"{count} ({100.0 * count / scored:.0f}" + backslash + "%)" for count in counts
            )
            name = DISPLAY_NAMES.get(tool, tool).replace("&", backslash + "&")
            lines.append(f"    {name} & {scored} & {cells} {row_end}")
        lines.append(r"    \addlinespace")
    lines = lines[:-1]
    lines += [r"    \bottomrule", r"  \end{tabular}", r"\end{table*}"]
    output_path.write_text("\n".join(lines) + "\n")
    print(f"Table written to {output_path}")


def write_node_bucket_table(accuracy_rows, output_path):
    r"""The same buckets as [`write_bucket_table`], but scored per *node* instead of per line.

    Only the tools whose output carries sub-line structure appear: Unix diff and the four git
    algorithms report whole lines and nothing finer, so `benchmark_other.rs` records them as
    `line_only` and they have no node column to bucket. That is exactly what makes this table worth
    having next to the line one - it is the only place BDiff's `str_diff` character offsets and
    Neovim's `DiffText` column runs are actually exercised, and both were scored `line_only` here
    until 2026-08-24 despite emitting them all along.

    The node metric is a "did the tool consider this node's text changed" projection, one
    granularity below the line columns - *not* node-to-node mapping fidelity, which cannot be asked
    of a tool that parses its own tree. See `benchmark_other.rs`'s `--accuracy-csv` doc comment.
    """
    backslash = "\\"
    row_end = backslash * 2
    header_labels = " & ".join(label for label, _ in BUCKETS)
    lines = [
        "% Auto-generated by research/analysis/benchmark_other_report.py. Do not edit by hand -",
        "% regenerate: make benchmark-timing-report (from research/).",
        r"\begin{table*}",
        (
            r"  \caption{Per-fixture agreement with the human mapping at \emph{node} granularity."
            r" Only tools whose output carries sub-line structure appear: a purely line-based tool"
            r" has no finer signal to project onto the AST, so it is recorded as line-only rather"
            r" than scored here. The metric is a per-node ``did you consider this changed''"
            r" projection, not mapping fidelity. ``Perfect'' means zero mismatched nodes, not a"
            r" rounded 100\%. Each tool is scored on its own applicable subset ($n$).}"
        ),
        r"  \label{tab:agreement-buckets-node}",
        r"  \small",
        r"  \begin{tabular}{lrrrrr}",
        r"    \toprule",
        f"    Tool & $n$ & {header_labels} {row_end}",
        r"    \midrule",
    ]
    for tool in GRANULARITY["subline"]:
        result = bucket_counts(accuracy_rows, tool, metric="node")
        if result is None:
            continue
        scored, counts = result
        cells = " & ".join(
            f"{count} ({100.0 * count / scored:.0f}" + backslash + "%)" for count in counts
        )
        name = DISPLAY_NAMES.get(tool, tool).replace("&", backslash + "&")
        lines.append(f"    {name} & {scored} & {cells} {row_end}")
    lines += [r"    \bottomrule", r"  \end{tabular}", r"\end{table*}"]
    output_path.write_text("\n".join(lines) + "\n")
    print(f"Table written to {output_path}")


def print_bucket_table(accuracy_rows):
    """The same buckets as plain text, for a terminal reader - line granularity, then node."""
    header = f"{'Tool':30}{'n':>6}" + "".join(f"{label:>14}" for label in PLAIN_BUCKET_LABELS)
    print("\nPer-fixture agreement, LINE granularity")
    print(header)
    print("-" * len(header))
    for group, members in GRANULARITY.items():
        print("line granularity only:" if group == "line" else "reports sub-line detail:")
        for tool in members:
            result = bucket_counts(accuracy_rows, tool)
            if result is None:
                continue
            scored, counts = result
            cells = "".join(f"{c:>7} ({100.0 * c / scored:>3.0f}%)" for c in counts)
            print(f"  {DISPLAY_NAMES.get(tool, tool):28}{scored:>6}{cells}")

    print("\nPer-fixture agreement, NODE granularity (line-only tools have no score here)")
    print(header)
    print("-" * len(header))
    for tool in GRANULARITY["subline"]:
        result = bucket_counts(accuracy_rows, tool, metric="node")
        if result is None:
            continue
        scored, counts = result
        cells = "".join(f"{c:>7} ({100.0 * c / scored:>3.0f}%)" for c in counts)
        print(f"  {DISPLAY_NAMES.get(tool, tool):28}{scored:>6}{cells}")


def plot_summary(rows, tools, output_path):
    """Median wall-clock per tool, one horizontal bar each.

    Accuracy is deliberately not here: `write_bucket_table` reports it, and a bucket table strictly
    dominates a bar of the pooled rate - it shows the same ordering plus the distribution behind
    it. This chart used to carry an accuracy panel too; it was dropped (2026-08-23) rather than
    maintained as a second, weaker view of numbers the table already carries.

    Speed is the median, not the mean: the per-tool means in this corpus are dominated by a
    cold-cache tail on whichever tool runs first per fixture (`unix_diff` and `git_myers` show
    means several times their medians, while the three later git variants - the same engine behind
    the same flag - do not). That is an artifact of run order, not of the algorithms.
    """
    ids = ordered(
        ["codediff"] + tools + [w for w in ("gumtree_warm", "bdiff_warm") if has_warm(rows, w)]
    )
    fig, ax = plt.subplots(figsize=(11, 5.5), facecolor=SURFACE)

    present = [(i, median_ms(rows, i)) for i in ids]
    present = [(i, v) for i, v in present if v is not None]
    present.sort(key=lambda pair: pair[1])
    names = [f"{DISPLAY_NAMES[i]}  (n={len(rows_for(i, rows))})" for i, _ in present]

    ax.set_facecolor(SURFACE)
    bars = ax.barh(
        range(len(present)),
        [v for _, v in present],
        color=[COLORS[i] for i, _ in present],
        zorder=3,
        height=0.72,
    )
    ax.set_yticks(range(len(present)))
    ax.set_yticklabels(names, fontsize=9.5, color=INK_SECONDARY)
    ax.invert_yaxis()
    ax.set_xscale("log")
    span = max(v for _, v in present) if present else 1
    for bar, (_, value) in zip(bars, present):
        ax.text(
            bar.get_width() * 1.06,
            bar.get_y() + bar.get_height() / 2,
            f"{value:,.1f} ms",
            va="center",
            ha="left",
            fontsize=9.5,
            color=INK_SECONDARY,
            zorder=4,
        )
    ax.set_xlim(right=span * 2.2)
    ax.set_xlabel("Milliseconds", fontsize=10.5, color=INK_SECONDARY)
    ax.set_title(
        "Median wall-clock per fixture (log scale, lower is better)",
        fontsize=12,
        color=INK_PRIMARY,
        loc="left",
        pad=10,
    )
    ax.tick_params(colors=INK_MUTED, labelsize=9)
    ax.grid(axis="x", color=GRIDLINE, linewidth=1, zorder=0)
    for spine in ("top", "right", "left"):
        ax.spines[spine].set_visible(False)
    ax.spines["bottom"].set_color(BASELINE)

    output_path.parent.mkdir(parents=True, exist_ok=True)
    fig.savefig(output_path, dpi=150, bbox_inches="tight", facecolor=SURFACE)
    plt.close(fig)
    print(f"Plot saved to {output_path}")


def median_ms(rows, id_):
    """Median of every individual repeat measurement for `id_`, or None if it has no timings."""
    values = [v for r in rows_for(id_, rows) for v in ms_values(r, f"{id_}_ms")]
    return float(np.median(values)) if values else None


def plot_runtime(rows: list[dict], tools: list[str], output_path: Path) -> None:
    """One violin per tool, plus a `treesitter_parse` reference violin and (when present in the
    CSV) a `gumtree_warm` reference violin, full per-fixture distribution rather than just the mean
    - codediff's 93 per-fixture times span ~3 orders of magnitude (3ms to 3.9s, dominated by file
    size), so a single bar hides exactly the shape that matters here. `violinplot` runs its KDE in
    log10-space (matplotlib's own kernel bandwidth assumes a linear axis, so feeding it raw ms on a
    log-scaled axis would misshape the KDE) - the y-axis ticks are then relabeled back to real ms.

    Series order and display names both come from `DISPLAY_ORDER`/`DISPLAY_NAMES` (see `ordered`),
    not from `tools`' own CSV-column order, so this plot, `plot_accuracy`, and
    `variance_table_rows` all read in the same fixed sequence regardless of which columns happen
    to appear first in the CSV: TreeSitter parse, UNIX diff, CodeDiff, GumTree (binary), GumTree
    (warm JVM), diffsitter, difftastic. `treesitter_parse` goes first: it's not a competing tool
    (no accuracy
    to score, see `plot_accuracy`'s doc comment for why it's absent there), it's the reference
    lower bound every AST-aware series to its right must pay before their own work even starts -
    reading left to right is reading the cost stack from the ground up.

    `gumtree_warm`, when present, goes right after `gumtree`: same algorithm, same accuracy, timed
    inside a persistent JVM instead of a fresh subprocess per fixture
    (`research/drivers/gumtree-batch/`) - it isolates GumTree's own cost from the JVM-startup
    overhead `gumtree`'s own violin includes. `benchmark_other.rs` only fills this column in when
    the batch driver was actually available for that run (see `gumtree_warm_batch`'s doc comment) -
    every cell blank means the whole column is absent, not scored per-fixture like `gumtree_ms` can
    be, so `tools` (built from `_mismatches` columns) never lists it and `has_gumtree_warm` checks
    for it separately.

    Each tool's violin is built from its own `applicable_rows` (see that function's doc comment) -
    a language-scoped tool like GumTree has far fewer points than codediff/unix_diff, so its violin
    is necessarily noisier and its x-tick carries an explicit "(n=...)" rather than implying the
    same sample size as everything else on the axis. `treesitter_parse_ms` has no scope gaps (every
    corpus language parses), so it's always full sample size, same as codediff.

    Every point plotted is one individual `--repeats` measurement, not a per-fixture mean/median -
    `ms_values` splits `benchmark_other.rs`'s `;`-joined column back into its full sample, so a
    fixture run with 3 repeats contributes 3 points to the violin/strip, not 1. This is what makes
    the shape here directly comparable to `variance_table_rows`' per-fixture coefficient of
    variation: both read from the same underlying repeats, just aggregated differently. `n` in
    each x-tick counts *fixtures*
    (matching `sample_sizes`' pre-existing meaning), not the larger flattened point count - the
    fixture count is what determines each violin's real independence (3 repeats of the same
    fixture are correlated with each other, not 3 independent fixtures), so it stays the honest
    number to report as "n"."""
    ids = ordered(
        ["treesitter_parse", "codediff"]
        + tools
        + [w for w in ("gumtree_warm", "bdiff_warm") if has_warm(rows, w)]
    )
    labels = [DISPLAY_NAMES[i] for i in ids]
    colors = [COLORS[i] for i in ids]
    scoped_rows = [rows_for(i, rows) for i in ids]
    sample_sizes = [len(s) for s in scoped_rows]
    series_ms = [
        np.array([v for r in s for v in ms_values(r, f"{i}_ms")]) for i, s in zip(ids, scoped_rows)
    ]

    series_log = [np.log10(s) for s in series_ms]

    # Per-item width bumped from 2 to 2.6 (2026-07-31): the longer display names ("TreeSitter parse
    # (lower bound)", "UNIX diff (baseline)") need more horizontal room per x-tick than the old
    # short ids did, or adjacent tick labels visually collide.
    fig, ax = plt.subplots(figsize=(3 + 2.6 * len(labels), 5.5), facecolor=SURFACE)
    ax.set_facecolor(SURFACE)

    x = np.arange(len(labels))
    rng = np.random.default_rng(0)
    for xi, log_vals in zip(x, series_log):
        # A light jittered strip under the violin - with only 93 points per tool, the KDE shape
        # alone can suggest more density than the raw points actually support.
        jitter = rng.uniform(-0.07, 0.07, size=len(log_vals))
        ax.scatter(
            np.full(len(log_vals), xi) + jitter,
            log_vals,
            s=10,
            alpha=0.35,
            color=INK_MUTED,
            linewidth=0,
            zorder=2,
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
            xi + 0.36,
            median_log,
            f"median {10**median_log:.2f} ms",
            ha="left",
            va="center",
            fontsize=9,
            color=INK_SECONDARY,
            zorder=4,
        )

    # Real-ms tick labels on the log10-transformed axis: 1, 3, 10, 30, ... spans the full range
    # (min ~2.3ms, max ~3.9s) in the familiar 1/3 log steps, not raw log10 values.
    tick_ms = [1, 3, 10, 30, 100, 300, 1000, 3000]
    tick_ms = [
        v
        for v in tick_ms
        if np.log10(v) >= min(s.min() for s in series_log) - 0.3
        and np.log10(v) <= max(s.max() for s in series_log) + 0.3
    ]
    ax.set_yticks(np.log10(tick_ms))
    ax.yaxis.set_major_formatter(ticker.FuncFormatter(lambda v, _: f"{10**v:,.3g}"))

    ax.set_xticks(x)
    ax.set_xticklabels(
        [f"{label}\n(n={n})" for label, n in zip(labels, sample_sizes)],
        fontsize=10,
        color=INK_MUTED,
    )
    # Extra room on the right for the last violin's median label, which sits past its own x
    # position (see the `ax.text` call above) rather than centered on the violin.
    ax.set_xlim(-0.6, len(labels) - 1 + 1.3)
    ax.set_ylabel("Time per fixture (ms, log scale)", fontsize=11, color=INK_SECONDARY)
    ax.set_title(
        "Runtime distribution: time to produce per-line touched/untouched labels (sample size varies by tool, see x-axis)",
        fontsize=13,
        color=INK_PRIMARY,
        loc="left",
        pad=12,
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


def coefficients_of_variation(rows: list[dict], column: str) -> np.ndarray:
    """Per-fixture coefficient of variation (stddev / mean, as a percentage) for `column`'s
    `--repeats` measurements - one value per fixture that had >= 2 repeats and a positive mean
    (a fixture run with `--repeats 1`, or a tool not applicable to it, contributes nothing: there's
    no spread to measure from a single sample). This is the same summary statistic `benchmark_
    other.rs`'s own `mean_coefficient_of_variation` prints in its console table, computed here
    directly from the CSV instead of trusting the console output, so this plot and that table can
    be cross-checked against each other."""
    out = []
    for r in rows:
        values = ms_values(r, column)
        if len(values) < 2:
            continue
        mean = float(np.mean(values))
        if mean <= 0:
            continue
        out.append(100.0 * float(np.std(values)) / mean)
    return np.array(out)


def _escape_tex(text: str) -> str:
    """Minimal LaTeX escaping for `DISPLAY_NAMES` values embedded in a generated `.tex` table - none
    of the current names need this (parentheses have no special meaning in LaTeX), but a future
    tool name with `_`/`%`/`&`/`#` would otherwise silently break the build."""
    for char in "&%$#_{}":
        text = text.replace(char, f"\\{char}")
    return text


def variance_table_rows(rows: list[dict], tools: list[str]) -> list[tuple[str, int, float, float]]:
    """Per-tool `(display_name, n, median_cov_pct, p90_cov_pct)` - the same
    `coefficients_of_variation` data `plot_variance`'s box plot used to draw, reduced to the two
    numbers that actually matter for "how much should one run's number be trusted": the typical
    case (median) and a worst-case-but-not-a-single-outlier case (p90). Same
    `DISPLAY_ORDER`/`DISPLAY_NAMES`/`ordered` sequence as `plot_runtime`/`plot_accuracy`. A tool
    scored on too few multi-repeat fixtures (e.g. GumTree on a corpus with only 1-2 fixtures in its
    language scope) can't support a meaningful median/p90 - dropped rather than reported from 1-2
    points, same spirit as `applicable_rows` dropping out-of-scope rows entirely instead of
    coercing them to a misleading value."""
    ids = ordered(
        ["treesitter_parse", "codediff"]
        + tools
        + [w for w in ("gumtree_warm", "bdiff_warm") if has_warm(rows, w)]
    )
    out = []
    for id_ in ids:
        series = coefficients_of_variation(rows_for(id_, rows), f"{id_}_ms")
        if len(series) < 3:
            continue
        out.append(
            (
                DISPLAY_NAMES[id_],
                len(series),
                float(np.median(series)),
                float(np.percentile(series, 90)),
            )
        )
    return out


def write_variance_table(rows: list[dict], tools: list[str], output_path: Path) -> None:
    """Writes a complete, ready-to-`\\input`-ed ACM `table` environment - see this module's own
    doc comment for why this is a table, not a plot, and `variance_table_rows` for the numbers."""
    table_rows = variance_table_rows(rows, tools)

    lines = [
        r"\begin{table}",
        (
            r"  \caption{Timing noise per series: per-fixture coefficient of variation across"
            r" repeated measurements of the same fixture (lower means a single run is more"
            r" trustworthy on its own).}"
        ),
        r"  \label{tab:variance}",
        r"  \begin{tabular}{lrrr}",
        r"    \toprule",
        r"    Series & $n$ & Median CoV & p90 CoV \\",
        r"    \midrule",
    ]
    for name, n, median, p90 in table_rows:
        lines.append(f"    {_escape_tex(name)} & {n} & {median:.1f}\\% & {p90:.1f}\\% \\\\")
    lines += [
        r"    \bottomrule",
        r"  \end{tabular}",
        r"\end{table}",
        "",
    ]

    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text("\n".join(lines))
    print(f"Table written to {output_path}")

    name_width = max(len(name) for name, _, _, _ in table_rows)
    print(f"{'Series':<{name_width}}  {'n':>4}  {'Median CoV':>11}  {'p90 CoV':>8}")
    for name, n, median, p90 in table_rows:
        print(f"{name:<{name_width}}  {n:>4}  {median:>10.1f}%  {p90:>7.1f}%")


# LaTeX macro stem for each series id, for `write_paper_fragment`. Deliberately a separate mapping
# from `DISPLAY_NAMES`: those are chart labels and may be reworded freely, while these are baked
# into `main.tex`'s macro names and into `paper_variables.py`'s expected-macro list, so renaming one
# is an edit across three files. Note `gumtree` -> `GumTreeCold`: the paper distinguishes GumTree
# invoked fresh per fixture (this CSV's `gumtree_ms`) from GumTree inside one persistent JVM
# (`gumtree_warm_ms`), and "cold" is the name the paper's own table uses for the former.
# Adding a tool here adds its macros to plots/variables_comparison.tex, and therefore to
# plots/variables.tex. It does *not* add a row to any table in the paper - main.tex references
# whichever macros its prose and tables actually use, and the generated-but-unreferenced ones are
# harmless (see the `Shape*` block for the established precedent). Every tool added 2026-08-23
# covers the full corpus, so `common_subset` below is unchanged by their presence.
PAPER_MACRO_STEMS = {
    "codediff": "CodeDiff",
    "unix_diff": "UnixDiff",
    "git_myers": "GitMyers",
    "git_minimal": "GitMinimal",
    "git_patience": "GitPatience",
    "git_histogram": "GitHistogram",
    "bdiff": "BDiff",
    "nvim_diff": "NvimDiff",
    "gumtree": "GumTree",
    "difftastic": "Difftastic",
    "diffsitter": "Diffsitter",
}
PAPER_SPEED_STEMS = PAPER_MACRO_STEMS | {
    "gumtree": "GumTreeCold",
    "gumtree_warm": "GumTreeWarm",
    "bdiff": "BDiffCold",
    "bdiff_warm": "BDiffWarm",
}


def read_accuracy_rows(csv_path: Path) -> list[dict] | None:
    """`benchmark_accuracy.csv`'s rows, or None if it isn't on disk. Optional by design - see this
    module's doc comment for why the speed half must not depend on the accuracy file existing."""
    if not csv_path.exists():
        return None
    with csv_path.open() as f:
        return list(csv.DictReader(f))


def accuracy_totals(rows: list[dict], tool: str) -> tuple[int, int, int] | None:
    """`(fixtures, line_mismatches, total_lines)` for `tool` over the fixtures it was actually
    scored on - `<tool>_status == "ok"`, so `unsupported` (no parser for that language) and `error`
    rows are excluded rather than counted as zero mismatches, which would make a narrowly-scoped
    tool look artificially perfect. Unix diff's `line_only` status is scored here too: that status
    describes its *node* columns, which this function does not read. Returns None for a tool with
    no scored fixtures at all."""
    scored = [r for r in rows if r.get(f"{tool}_status") in ("ok", "line_only")]
    if not scored:
        return None
    mismatches = sum(int(r[f"{tool}_line_mismatches"]) for r in scored)
    total = sum(int(r["total_lines"]) for r in scored)
    return len(scored), mismatches, total


def common_subset(rows: list[dict], tools: list[str]) -> list[dict]:
    """The fixtures *every* tool in `tools` scored. The per-tool table each tool's own applicable
    subset produces is not apples-to-apples - diffsitter's compiled-in grammar set covers barely
    half the corpus, GumTree's rather more - so a tool's headline rate partly reflects which
    languages it happens to cover rather than how well it diffs them. This subset holds the
    fixture set fixed across all five, at the cost of being biased toward the mainstream languages
    every tool supports."""
    return [r for r in rows if all(r.get(f"{t}_status") in ("ok", "line_only") for t in tools)]


def speed_percentiles(rows: list[dict], id_: str) -> tuple[float, float, float] | None:
    """`(p50, p90, p99)` in milliseconds over *every individual repeat* of `id_`, pooled across
    fixtures - the same population `plot_runtime` draws, so the table and the figure cannot
    disagree. Pooling is sound here because `benchmark_other` runs a uniform `--repeats` for every
    fixture and tool, so no fixture is over-weighted; that would stop being true if repeats ever
    became adaptive (as they are in `benchmark_diff_pairs`, which skips repeats for already-slow
    pairs). Returns None for a series with no measurements."""
    values = [v for r in rows_for(id_, rows) for v in ms_values(r, f"{id_}_ms")]
    if not values:
        return None
    array = np.array(values, dtype=float)
    return tuple(float(np.percentile(array, p)) for p in (50, 90, 99))


def write_paper_fragment(
    rows: list[dict],
    tools: list[str],
    accuracy_rows: list[dict] | None,
    output_path: Path,
) -> None:
    """Writes the comparison and speed numbers the introductory paper cites as LaTeX macros. See
    this module's doc comment for why this exists and which CSV each half comes from."""
    lines = [
        "% Auto-generated by research/analysis/benchmark_other_report.py. Do not edit by hand -",
        "% regenerate: make benchmark-timing-report (from research/). Merged into",
        "% plots/variables.tex by analysis/paper_variables.py; see that script's module doc comment.",
    ]

    if accuracy_rows is None:
        print(
            "note: no benchmark_accuracy.csv - writing speed macros only (run `make benchmark-accuracy`)."
        )
    else:
        lines.append(f"% Accuracy: {len(accuracy_rows)} fixtures with a human mapping.")
        for id_ in ordered(list(PAPER_MACRO_STEMS)):
            totals = accuracy_totals(accuracy_rows, id_)
            if totals is None:
                continue
            fixtures, mismatches, total = totals
            stem = PAPER_MACRO_STEMS[id_]
            lines.append(f"\\newcommand{{\\{stem}Fixtures}}{{{fixtures}}}")
            lines.append(
                f"\\newcommand{{\\{stem}LineMismatches}}{{{mismatches:,}}}".replace(",", "{,}")
            )
            lines.append(f"\\newcommand{{\\{stem}LineRate}}{{{100.0 * mismatches / total:.3f}}}")

        shared = common_subset(accuracy_rows, list(PAPER_MACRO_STEMS))
        lines.append(f"% Common subset: the {len(shared)} fixtures every tool scored.")
        lines.append(f"\\newcommand{{\\CommonFixtures}}{{{len(shared)}}}")
        for id_ in ordered(list(PAPER_MACRO_STEMS)):
            totals = accuracy_totals(shared, id_)
            if totals is None:
                continue
            _, mismatches, total = totals
            lines.append(
                f"\\newcommand{{\\Common{PAPER_MACRO_STEMS[id_]}LineRate}}{{{100.0 * mismatches / total:.3f}}}"
            )

    lines.append("% Speed: pooled per-repeat wall-clock, milliseconds.")
    speed_ids = ordered(
        ["codediff"] + tools + [w for w in ("gumtree_warm", "bdiff_warm") if has_warm(rows, w)]
    )
    for id_ in speed_ids:
        percentiles = speed_percentiles(rows, id_)
        if percentiles is None or id_ not in PAPER_SPEED_STEMS:
            continue
        stem = PAPER_SPEED_STEMS[id_]
        for name, value in zip(("PFifty", "PNinety", "PNinetyNine"), percentiles):
            lines.append(f"\\newcommand{{\\Speed{stem}{name}}}{{{value:.1f}}}")

    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text("\n".join(lines) + "\n")
    macro_count = sum(1 for line in lines if line.startswith("\\newcommand"))
    print(f"{macro_count} paper comparison variables written to {output_path}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(
        description="Compare codediff against other diff tools from benchmark_other.csv."
    )
    parser.add_argument(
        "--csv",
        default="data/comparison/benchmark_other.csv",
        help="Path to the benchmark CSV (default: data/comparison/benchmark_other.csv)",
    )
    parser.add_argument(
        "--accuracy-csv",
        default="data/comparison/benchmark_accuracy.csv",
        help="Path to the accuracy CSV (default: data/comparison/benchmark_accuracy.csv)",
    )
    parser.add_argument(
        "--plots-dir", default="plots", help="Directory for output PNGs (default: plots/)"
    )
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
    # The bucketed-agreement histogram was removed 2026-08-23 - see `write_bucket_table` for why
    # it could not show the result it existed to show. `plot_runtime` stays: its data spans three
    # orders of magnitude and it is the figure that exposes BDiff's and GumTree's cold/warm split.
    plot_summary(rows, tools, plots_dir / "benchmark_other_summary.png")
    plot_runtime(rows, tools, plots_dir / "benchmark_other_runtime.png")
    write_variance_table(rows, tools, plots_dir / "benchmark_other_variance.tex")
    accuracy_rows = read_accuracy_rows(Path(args.accuracy_csv))
    if accuracy_rows:
        # Two fragments, same reason `plot_accuracy`/`plot_accuracy_with_codediff` were two files:
        # the paper answers RQ2 without codediff, and a reader asking where codediff lands wants
        # it in. Neither audience can be handed the other's table by accident.
        write_bucket_table(
            accuracy_rows, plots_dir / "benchmark_other_buckets.tex", include_codediff=False
        )
        write_bucket_table(
            accuracy_rows,
            plots_dir / "benchmark_other_buckets_with_codediff.tex",
            include_codediff=True,
        )
        write_node_bucket_table(accuracy_rows, plots_dir / "benchmark_other_buckets_node.tex")
        print_bucket_table(accuracy_rows)

    write_paper_fragment(
        rows,
        tools,
        accuracy_rows,
        plots_dir / "variables_comparison.tex",
    )
