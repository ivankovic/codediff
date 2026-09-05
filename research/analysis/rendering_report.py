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
What the ground truth says about the *rendered* diff: how much of an AST ever reaches the screen,
and how often the text that reaches it has more than one correct rendering.

Two measurements, reported together because the paper uses them as one argument. The first bounds
what a syntax-aware diff has to get right - a mapping error on a node that is never displayed
cannot mislead a reader. The second shows that even once the question is narrowed to what is
displayed, the correct answer is still not always unique.

=== Visibility ===

`diff::nodes::structurally_visible_node_ids` marks the nodes that carry text of their own, the
ones a rendered diff can display. Its doc comment states the property this report needs: the set
"depends only on `code`", never on any diff of it, so the fraction below is a property of the
corpus's syntax trees rather than of any tool that walks them. That is what lets this number stand
in a section that names no tool. Per-fixture counts come from
data/quality/optimal_solutions_benchmark.csv, whose `visible_nodes` and `total_nodes` columns are
that function applied to each fixture's two files.

=== Rendering ambiguity ===

This half is the rendering-level companion to `ambiguity_report.py`. That script measures how often the
node *mapping* admits several equally correct answers; this one measures how often the painted
*text* does. The two are different questions about the same corpus, and a tool can fail either
independently: it can recover the right mapping and render it in a way no human would, or recover
a defensible rendering from a mapping that is wrong underneath.

=== What a painting is, and why a *count* of them is the measurement ===

`human_mapping.json`'s `text_mappings` field (`NamedTextMapping` in src/test/helper/human_mapping.rs)
holds zero or more named paintings, each a list of `HumanTextEntry` spans labelled Move, Update,
Insert or Delete. `human_solver` offers three names - `Minimal`, `Full`, `Only one solution` - but
the name is a label, not the datum. The datum is how many paintings a fixture carries, and
`painting_for_mode`'s doc comment states the semantics this report depends on:

  * ONE painting is a positive assertion. The painter looked at the change and judged that it has
    a single defensible rendering, so both render modes are held to that one answer.
  * TWO paintings assert that both are correct renderings of the same edit, exactly as a
    `MultiMapGroup` asserts that any consistent pairing of its members is correct. The fixture's
    validation accepts whichever the tool came closest to.

So neither cell of the split is a gap in the data. A fixture with one painting is not one that
nobody got round to painting twice; it is one an annotator examined and found unambiguous.

=== Corpus state, and the population this rate is *not* over ===

Scoped to the fixtures listed in data/quality/human_mapping_analysis.csv, the corpus state every
other number in the paper's evaluation is measured against, for the reason `ambiguity_report.py`
gives at length: a rate whose denominator drifts from the rest of the paper's is not comparable
with any of them.

Within that scope the painted set is *not* a random sample, and the report prints the evidence
rather than leaving a reader to assume otherwise. Painting is manual and slow, and it was done on
the small curated `handmade` cases first, so for a long time *every* painted fixture was one of
those. That is no longer true: as of 2026-09-05 the sampled datasets carry the majority of the
painted set, and the two populations answer the question differently enough that a single
aggregate rate hides it. So the split is emitted as macros of its own (`PaintingHandmade*` /
`PaintingSampled*`) alongside the aggregate, together with the characterisation (category split,
median size, language coverage), so the paper states what the rate is over rather than implying
a population it no longer has.

Usage (from research/):
    uv run ./analysis/rendering_report.py
    uv run ./analysis/rendering_report.py --plots-dir plots/
"""

import argparse
import csv
import json
import statistics
from collections import Counter
from pathlib import Path

DIFFS_ROOT = "src/test/data/diffs"

# A span holding exactly one of these, and nothing else, is what finding 1 of
# data/quality/text_painting_findings.md counts as "a single punctuation token" when it accounts
# for the difference between a Minimal and a Full painting of the same change.
PUNCTUATION = set("()[]{}<>,;:.=+-*/&|!?%^~'\"`@#$\\")


def repo_root(here: Path) -> Path:
    return here.parent.parent


def pct(part: int, whole: int) -> float:
    return 100.0 * part / whole if whole else 0.0


def fixture_rows_in_scope(csv_path: Path) -> dict[str, dict]:
    """The fixture set the paper's evaluation is measured against, keyed by fixture name."""
    with open(csv_path, newline="") as f:
        return {r["name"]: r for r in csv.DictReader(f) if r["has_mapping"] == "true"}


def fixture_sources(fixture_dir: Path) -> dict[str, str]:
    """The fixture's `before`/`after` text, keyed by side.

    The extension varies by language (`before.rs.test`, `before.h.test`, ...), so the side is read
    off the stem rather than matched against a known extension list.
    """
    sources: dict[str, str] = {}
    for path in fixture_dir.glob("*.test"):
        side = path.name.split(".", 1)[0]
        if side in ("before", "after"):
            sources[side] = path.read_text(encoding="utf-8", errors="replace")
    return sources


def spans(entry: dict, side: str) -> list[dict]:
    """A side's spans, normalising the two on-disk shapes into one.

    `HumanTextEntry`'s `spans_from_one_or_many` accepts either a bare span object or a list of
    them: the fields held exactly one span before N:M text matches existed, and fixtures painted
    against that schema are committed unchanged. A bare object is exactly a one-element list.
    """
    value = entry.get(side)
    if not value:
        return []
    return value if isinstance(value, list) else [value]


def span_text(source: str, span: dict) -> str:
    """The source text a painted span covers. Empty when the span is out of range for the file."""
    lines = source.splitlines()
    start_row, end_row = span["start_row"], span["end_row"]
    if start_row >= len(lines) or end_row >= len(lines):
        return ""
    if start_row == end_row:
        return lines[start_row][span["start_column"] : span["end_column"]]
    parts = [lines[start_row][span["start_column"] :]]
    parts += lines[start_row + 1 : end_row]
    parts.append(lines[end_row][: span["end_column"]])
    return "\n".join(parts)


def is_single_punctuation(text: str) -> bool:
    stripped = text.strip()
    return bool(stripped) and all(c in PUNCTUATION for c in stripped)


def visibility(csv_path: Path, rows: dict[str, dict]) -> dict:
    """Visible-node share per fixture, pooled and as a distribution.

    Both readings are reported because they can disagree: a pooled ratio is dominated by the few
    largest fixtures, while the per-fixture median weights every change equally. Where they agree,
    as they do here, that agreement is itself the result - it says the share is a property of
    source code generally rather than of a handful of outliers.
    """
    visible = total = 0
    fractions: list[float] = []
    with open(csv_path, newline="") as f:
        for row in csv.DictReader(f):
            if row["solution"] not in rows:
                continue
            node_count = int(row["total_nodes"])
            if not node_count:
                continue
            visible += int(row["visible_nodes"])
            total += node_count
            fractions.append(100.0 * int(row["visible_nodes"]) / node_count)
    fractions.sort()
    return {
        "visible_nodes": visible,
        "total_nodes": total,
        "visible_fixtures": len(fractions),
        "visible_median": statistics.median(fractions) if fractions else 0.0,
        "visible_p10": fractions[len(fractions) // 10] if fractions else 0.0,
        "visible_p90": fractions[9 * len(fractions) // 10] if fractions else 0.0,
    }


def collect(root: Path, rows: dict[str, dict]) -> dict:
    """Every painting-derived number the paper quotes, in one pass over the mapping files."""
    painted: dict[str, list[dict]] = {}
    sources: dict[str, dict[str, str]] = {}
    for path in sorted((root / DIFFS_ROOT).glob("*/*/human_mapping.json")):
        name = path.parent.name
        if name not in rows:
            continue
        mappings = json.loads(path.read_text()).get("text_mappings") or []
        if mappings:
            painted[name] = mappings
            sources[name] = fixture_sources(path.parent)

    single = [n for n, m in painted.items() if len(m) == 1]
    dual = [n for n, m in painted.items() if len(m) > 1]

    # Entry-level totals, over every painting of every painted fixture.
    operations: Counter[str] = Counter()
    match_entries = 0
    nm_entries = 0
    nm_shapes: Counter[str] = Counter()
    for name, mappings in painted.items():
        for mapping in mappings:
            for entry in mapping["entries"]:
                operations[entry["operation"]] += 1
                before, after = spans(entry, "before"), spans(entry, "after")
                if before and after:
                    match_entries += 1
                    if len(before) > 1 or len(after) > 1:
                        nm_entries += 1
                        nm_shapes[f"{len(before)}:{len(after)}"] += 1

    # What separates the two paintings of a doubly-painted fixture. Finding 1 of
    # data/quality/text_painting_findings.md: the Minimal/Full fork is largely about punctuation.
    minimal_entries = full_entries = 0
    extra_spans = extra_punctuation = 0
    for name in dual:
        by_name = {m["name"]: m for m in painted[name]}
        minimal, full = by_name.get("Minimal"), by_name.get("Full")
        if not minimal or not full:
            continue
        minimal_entries += len(minimal["entries"])
        full_entries += len(full["entries"])
        # An entry of Full's that no entry of Minimal's covers the same span on the same side.
        minimal_spans = {
            (side, json.dumps(span, sort_keys=True))
            for entry in minimal["entries"]
            for side in ("before", "after")
            for span in spans(entry, side)
        }
        for entry in full["entries"]:
            for side in ("before", "after"):
                for span in spans(entry, side):
                    key = (side, json.dumps(span, sort_keys=True))
                    if key in minimal_spans:
                        continue
                    extra_spans += 1
                    if is_single_punctuation(span_text(sources[name].get(side, ""), span)):
                        extra_punctuation += 1

    def loc(names: list[str]) -> list[int]:
        return [int(rows[n]["before_loc"]) for n in names]

    unpainted = [n for n in rows if n not in painted]

    # The painted set splits into the hand-written examples painting started on and the fixtures
    # captured from real commits. Reported separately because the aggregate rate is a mixture of
    # two very different ones, and the mixing proportion is an artifact of annotation order.
    handmade = [n for n in painted if rows[n]["category"] == "handmade"]
    sampled = [n for n in painted if rows[n]["category"] != "handmade"]
    dual_set = set(dual)
    return {
        "scored": len(rows),
        "painted": len(painted),
        "single": len(single),
        "dual": len(dual),
        "handmade_painted": len(handmade),
        "handmade_dual": len([n for n in handmade if n in dual_set]),
        "sampled_painted": len(sampled),
        "sampled_dual": len([n for n in sampled if n in dual_set]),
        "categories": Counter(rows[n]["category"] for n in painted),
        "category_totals": Counter(r["category"] for r in rows.values()),
        "languages": len({rows[n]["language"] for n in painted}),
        "corpus_languages": len({r["language"] for r in rows.values()}),
        "painted_loc_median": statistics.median(loc(list(painted))) if painted else 0,
        "painted_loc_max": max(loc(list(painted))) if painted else 0,
        "unpainted_loc_median": statistics.median(loc(unpainted)) if unpainted else 0,
        "operations": operations,
        "entries": sum(operations.values()),
        "match_entries": match_entries,
        "nm_entries": nm_entries,
        "nm_shapes": nm_shapes,
        "minimal_entries": minimal_entries,
        "full_entries": full_entries,
        "extra_spans": extra_spans,
        "extra_punctuation": extra_punctuation,
        "single_names": sorted(single),
        "dual_names": sorted(dual),
    }


def report(s: dict) -> None:
    print(
        f"Painted fixtures: {s['painted']} of {s['scored']} in scope "
        f"({pct(s['painted'], s['scored']):.1f}%)"
    )
    print(
        f"  hand-written: {s['handmade_dual']}/{s['handmade_painted']} dual "
        f"({pct(s['handmade_dual'], s['handmade_painted']):.1f}%); "
        f"sampled from real commits: {s['sampled_dual']}/{s['sampled_painted']} dual "
        f"({pct(s['sampled_dual'], s['sampled_painted']):.1f}%)"
    )
    print(
        f"  one painting  (rendering judged unique): {s['single']} "
        f"({pct(s['single'], s['painted']):.1f}%)"
    )
    print(
        f"  two paintings (both correct):            {s['dual']} "
        f"({pct(s['dual'], s['painted']):.1f}%)"
    )
    print("\nWhat the painted set is (it is not a random sample of the corpus):")
    for category, count in s["categories"].most_common():
        print(f"  {category}: {count} painted of {s['category_totals'][category]} in the corpus")
    for category, total in s["category_totals"].most_common():
        if category not in s["categories"]:
            print(f"  {category}: 0 painted of {total} in the corpus")
    print(f"  languages: {s['languages']} of the corpus's {s['corpus_languages']}")
    print(
        f"  median LOC: painted {s['painted_loc_median']:.0f} (max {s['painted_loc_max']}), "
        f"unpainted {s['unpainted_loc_median']:.0f}"
    )
    print(f"\nPainted entries: {s['entries']}")
    for operation, count in s["operations"].most_common():
        print(f"  {operation}: {count}")
    print(
        f"\nN:M text correspondences: {s['nm_entries']} of {s['match_entries']} matched entries "
        f"({pct(s['nm_entries'], s['match_entries']):.1f}%)"
    )
    for shape, count in sorted(s["nm_shapes"].items()):
        print(f"  {shape}: {count}")
    print(
        f"\nVisible nodes (a property of the trees, not of any diff): "
        f"{s['visible_nodes']:,} of {s['total_nodes']:,} pooled = "
        f"{pct(s['visible_nodes'], s['total_nodes']):.1f}%"
    )
    print(
        f"  per fixture: median {s['visible_median']:.1f}%, "
        f"p10 {s['visible_p10']:.1f}%, p90 {s['visible_p90']:.1f}%"
    )
    print(f"\nMinimal against Full, over the {s['dual']} doubly-painted fixtures:")
    print(f"  Minimal entries: {s['minimal_entries']}")
    print(f"  Full entries:    {s['full_entries']}")
    print(
        f"  spans Full paints that Minimal does not: {s['extra_spans']}, of which "
        f"{s['extra_punctuation']} ({pct(s['extra_punctuation'], s['extra_spans']):.0f}%) hold "
        f"punctuation alone"
    )


def write_paper_fragment(s: dict, output_path: Path) -> None:
    """Writes the painting numbers as LaTeX macros, same contract as ambiguity_report.py's.

    Merged into plots/variables.tex by analysis/paper_variables.py; regenerate with
    `make rendering-report` (from research/). Rates carry no percent sign - the paper adds \\%.
    """
    macros = {
        # Visibility: what share of an AST a rendered diff can display at all.
        "VisibleNodeShare": f"{pct(s['visible_nodes'], s['total_nodes']):.1f}",
        "InvisibleNodeShare": f"{100 - pct(s['visible_nodes'], s['total_nodes']):.1f}",
        "VisibleNodeShareMedian": f"{s['visible_median']:.1f}",
        "VisibleNodeSharePTen": f"{s['visible_p10']:.1f}",
        "VisibleNodeSharePNinety": f"{s['visible_p90']:.1f}",
        # Rendering ambiguity: how often the painted text admits more than one correct answer.
        "PaintingScored": s["scored"],
        "PaintingFixtures": s["painted"],
        "PaintingPct": f"{pct(s['painted'], s['scored']):.1f}",
        "PaintingSingle": s["single"],
        "PaintingSinglePct": f"{pct(s['single'], s['painted']):.1f}",
        "PaintingDual": s["dual"],
        "PaintingDualPct": f"{pct(s['dual'], s['painted']):.1f}",
        # The two populations behind that aggregate - see `collect`.
        "PaintingHandmadePainted": s["handmade_painted"],
        "PaintingHandmadeDual": s["handmade_dual"],
        "PaintingHandmadeDualPct": f"{pct(s['handmade_dual'], s['handmade_painted']):.1f}",
        "PaintingSampledPainted": s["sampled_painted"],
        "PaintingSampledDual": s["sampled_dual"],
        "PaintingSampledDualPct": f"{pct(s['sampled_dual'], s['sampled_painted']):.1f}",
        # What the painted set is, so the paper can state the population rather than imply one.
        "PaintingHandmadeTotal": s["category_totals"]["handmade"],
        "PaintingLanguages": s["languages"],
        "PaintingLocMedian": f"{s['painted_loc_median']:.0f}",
        "PaintingLocMax": s["painted_loc_max"],
        "PaintingUnpaintedLocMedian": f"{s['unpainted_loc_median']:.0f}",
        # Entry-level shape of what was painted.
        "PaintingEntries": s["entries"],
        "PaintingMatchEntries": s["match_entries"],
        "PaintingNmEntries": s["nm_entries"],
        "PaintingNmPct": f"{pct(s['nm_entries'], s['match_entries']):.1f}",
        # The Minimal/Full fork, and how much of it is punctuation.
        "PaintingMinimalEntries": s["minimal_entries"],
        "PaintingFullEntries": s["full_entries"],
        "PaintingFullExtraSpans": s["extra_spans"],
        "PaintingFullExtraPunctuation": s["extra_punctuation"],
        "PaintingFullExtraPunctuationPct": f"{pct(s['extra_punctuation'], s['extra_spans']):.0f}",
    }
    lines = [
        "% Auto-generated by research/analysis/rendering_report.py. Do not edit by hand -",
        "% regenerate: make painting-report (from research/). Merged into plots/variables.tex",
        "% by analysis/paper_variables.py; see that script's module doc comment.",
    ]
    lines += [f"\\newcommand{{\\{k}}}{{{v}}}" for k, v in macros.items()]
    output_path.write_text("\n".join(lines) + "\n")
    print(f"\nWrote {len(macros)} macros to {output_path}")


def main() -> None:
    parser = argparse.ArgumentParser(
        description="What the ground truth says about the rendered diff: how much of an AST is "
        "visible at all, and how often the visible text has more than one correct rendering."
    )
    here = Path(__file__).resolve().parent
    root = repo_root(here)
    parser.add_argument(
        "--csv",
        type=Path,
        default=root / "research/data/quality/human_mapping_analysis.csv",
        help="defines which fixtures are in scope (the paper's corpus state)",
    )
    parser.add_argument(
        "--benchmark-csv",
        type=Path,
        default=root / "research/data/quality/optimal_solutions_benchmark.csv",
        help="per-fixture visible_nodes/total_nodes, for the visibility half of this report",
    )
    parser.add_argument("--plots-dir", type=Path, default=root / "research/plots")
    args = parser.parse_args()

    rows = fixture_rows_in_scope(args.csv)
    stats = collect(root, rows) | visibility(args.benchmark_csv, rows)
    report(stats)
    args.plots_dir.mkdir(parents=True, exist_ok=True)
    write_paper_fragment(stats, args.plots_dir / "variables_rendering.tex")


if __name__ == "__main__":
    main()
