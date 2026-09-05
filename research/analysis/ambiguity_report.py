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
"""How often does a real-world code change admit *no unique* correct AST mapping?

This is the introductory paper's RQ3. It is a property of the ground-truth corpus alone - no
diffing tool appears in any number this script produces.

The measurement is `human_mapping.json`'s `groups` field (`MultiMapGroup` in
src/test/helper/human_mapping.rs): a set of N before-side nodes that may pair with M after-side
nodes in *any* consistent pairing, recorded by the annotator when several candidate nodes are
genuinely interchangeable. It does not mean the annotator was unsure - it means the annotator
established that more than one pairing is equally correct, and the fixture's validation accepts
any of them.

=== One corpus-wide rate, and why there is no longer a split ===

`groups` was added to the annotation tool partway through corpus construction, so for a while this
script reported three rates instead of one: fixtures whose mapping file had not been touched since
the facility landed were held apart from those that had, on the ground that the former could not
have recorded an ambiguity even where one existed. That split was a statement about git history,
not about the corpus, and it is no longer true of it: **every fixture has since been reviewed by an
annotator with multi-mapping available**. A review that found nothing to add leaves the file
untouched, so "last modified before date D" stopped being evidence of anything about the
annotation, and reporting it as though it were understated the corpus by counting reviewed
fixtures as unexamined ones.

So there is one rate, over every fixture in scope. It remains a lower bound, for the reason it
always did and the only one that survives: an annotator can miss an ambiguity that is really there.

=== Corpus state ===

Reads the mapping JSON files directly (they are the ground truth), but restricted to the fixtures
listed in data/quality/human_mapping_analysis.csv, which is what defines the corpus state every
other Section 5 number was measured against. Fixtures added since that CSV was written are
excluded rather than silently mixed in, because the paper's README requires Section 5's numbers to
describe one corpus state. Refresh order when fixtures are added: `analyze_human_mappings --csv`,
then this script.

Usage (from research/):
    uv run ./analysis/ambiguity_report.py
    uv run ./analysis/ambiguity_report.py --plots-dir plots/
"""

import argparse
import csv
import json
import statistics
import subprocess
from collections import Counter
from pathlib import Path

# Path prefix every fixture directory lives under, relative to the repository root.
DIFFS_ROOT = "src/test/data/diffs"


def repo_root(here: Path) -> Path:
    return here.parent.parent


def fixture_names_in_scope(csv_path: Path) -> set[str]:
    """The fixture set Section 5 is measured against - see this script's "Corpus state" note."""
    with open(csv_path, newline="") as f:
        return {r["name"] for r in csv.DictReader(f) if r["has_mapping"] == "true"}


def read_groups(root: Path, names: set[str]) -> dict[str, list[dict]]:
    """Fixture name -> its `groups` list (possibly empty), for every in-scope fixture."""
    tracked = subprocess.run(
        ["git", "ls-files", DIFFS_ROOT],
        capture_output=True,
        text=True,
        cwd=root,
        check=True,
    ).stdout.split()
    out = {}
    for path in tracked:
        if not path.endswith("human_mapping.json"):
            continue
        name = path.split("/")[-2]
        if name not in names:
            continue
        with open(root / path) as f:
            out[name] = json.load(f).get("groups", [])
    return out


def pct(part: int, whole: int) -> float:
    return 100.0 * part / whole if whole else 0.0


def paired_decisions(csv_path: Path, names: set[str]) -> int:
    """How many 1:1 *pairings* the ground truth records in the changed region of the corpus.

    `op_update` + `op_match_but_not_identical`: the entries where the annotator matched a before
    node to an after node and the two are not simply identical - i.e. the same kind of decision a
    multi-map group expresses, which is what makes it the right denominator for the group pairs.
    Deliberately excludes `op_identical` (overwhelmingly untouched scaffolding, and mostly implicit
    rather than an entry at all - see `node_ops` in analyze_human_mappings.rs) and the
    delete/insert operations, which are not pairings.
    """
    with open(csv_path, newline="") as f:
        return sum(
            int(r["op_update"] or 0) + int(r["op_match_but_not_identical"] or 0)
            for r in csv.DictReader(f)
            if r["has_mapping"] == "true" and r["name"] in names
        )


def datasets_for(root: Path, names: set[str]) -> dict[str, str]:
    """Fixture name -> the dataset directory it lives in (`handmade`, `small`, `full`, ...).

    Section 4 discusses the Curated and Full repository lists separately, which are the `small` and
    `full` directories here (see src/test/helper.rs's DIFF_DATASETS)."""
    out = {}
    for dataset in ("handmade", "small", "full", "stratified"):
        base = root / DIFFS_ROOT / dataset
        if not base.is_dir():
            continue
        for entry in base.iterdir():
            if entry.is_dir() and entry.name in names:
                out[entry.name] = dataset
    return out


def summarize(
    groups_by_fixture: dict[str, list[dict]],
    paired: int,
    datasets: dict[str, str] | None = None,
) -> dict:
    """Every number the paper's RQ3 quotes, in one dict."""
    names = sorted(groups_by_fixture)
    with_groups = [n for n in names if groups_by_fixture[n]]

    # Per repository list, over every fixture in it. Section 4 compares the Curated and Full lists,
    # and the raw rate is what supports that now: both lists have been annotated end to end by
    # someone with multi-mapping available, so a difference between them is a difference in the
    # code, not in when the files happened to be written.
    per_list = {}
    for label, dataset in (("Curated", "small"), ("Full", "full")):
        members = [n for n in names if (datasets or {}).get(n) == dataset]
        per_list[label] = {
            "total": len(members),
            "with": len([n for n in members if groups_by_fixture[n]]),
        }

    all_groups = [g for n in with_groups for g in groups_by_fixture[n]]
    sizes = [max(len(g["before_paths"]), len(g["after_paths"])) for g in all_groups]
    shapes = Counter((len(g["before_paths"]), len(g["after_paths"])) for g in all_groups)
    # The dominant shape family: N candidates on one side, N-1 or fewer on the other, at the
    # smallest size - one of two interchangeable nodes surviving, or one being added to a pair.
    minority = shapes[(2, 1)] + shapes[(1, 2)]

    # Pair-weighted reading, alongside the fixture-weighted one: how many of the ground truth's
    # own pairing decisions have no unique answer. `min(N, M)` is exactly how many pairs a group
    # asserts (see MultiMapGroup), and those pairs are not in `paired` - groups are counted
    # separately from `entries` - so the denominator is the sum, not `paired` alone.
    ambiguous_pairs = sum(min(len(g["before_paths"]), len(g["after_paths"])) for g in all_groups)

    return {
        "paired_decisions": paired + ambiguous_pairs,
        "ambiguous_pairs": ambiguous_pairs,
        "scored": len(names),
        "any_fixtures": len(with_groups),
        "any_pct": pct(len(with_groups), len(names)),
        "per_list": per_list,
        "groups": len(all_groups),
        "with_children": sum(1 for g in all_groups if g.get("with_children")),
        "op_identical": sum(1 for g in all_groups if g["operation"] == "identical"),
        "op_match": sum(1 for g in all_groups if g["operation"] == "match_but_not_identical"),
        "size_median": statistics.median(sizes) if sizes else 0,
        "size_max": max(sizes) if sizes else 0,
        "unequal_pct": pct(
            sum(1 for g in all_groups if len(g["before_paths"]) != len(g["after_paths"])),
            len(all_groups),
        ),
        "minority_pct": pct(minority, len(all_groups)),
        "max_groups_in_fixture": max((len(groups_by_fixture[n]) for n in with_groups), default=0),
        "shapes": shapes,
        "with_groups_names": with_groups,
    }


def write_paper_fragment(s: dict, output_path: Path) -> None:
    """Writes RQ3's numbers as LaTeX macros, same contract as apted_only_report.py's fragment.

    Merged into plots/variables.tex by analysis/paper_variables.py; regenerate with
    `make ambiguity-report` (from research/). Rates carry no percent sign - the paper adds \\%.
    """
    per_list = s.get("per_list", {})
    macros = {
        # Corpus-wide, over every fixture in scope - see this script's doc comment for why there
        # is one rate here rather than a split by when the mapping file was last written.
        "AmbiguityScored": s["scored"],
        "AmbiguityAnyFixtures": s["any_fixtures"],
        "AmbiguityAnyPct": f"{s['any_pct']:.1f}",
        # What the ambiguous cases look like.
        "AmbiguityGroups": s["groups"],
        "AmbiguityGroupsWithChildren": s["with_children"],
        "AmbiguityIdenticalGroups": s["op_identical"],
        "AmbiguityMatchGroups": s["op_match"],
        "AmbiguityGroupSizeMedian": int(s["size_median"]),
        "AmbiguityGroupSizeMax": s["size_max"],
        "AmbiguityUnequalPct": f"{s['unequal_pct']:.1f}",
        "AmbiguityMinorityPct": f"{s['minority_pct']:.1f}",
        "AmbiguityMaxGroupsInFixture": s["max_groups_in_fixture"],
        # Pair-weighted: the second reading RA3 quotes alongside the fixture-weighted rate.
        "AmbiguityPairs": s["ambiguous_pairs"],
        "AmbiguityPairedDecisions": f"{s['paired_decisions']:,}",
        "AmbiguityPairsPct": f"{pct(s['ambiguous_pairs'], s['paired_decisions']):.1f}",
    }
    lines = [
        "% Auto-generated by research/analysis/ambiguity_report.py. Do not edit by hand -",
        "% regenerate: make ambiguity-report (from research/). Merged into plots/variables.tex",
        "% by analysis/paper_variables.py; see that script's module doc comment.",
    ]
    # Per repository list - see `summarize`.
    for label, stats in sorted(per_list.items()):
        macros[f"Ambiguity{label}Total"] = stats["total"]
        macros[f"Ambiguity{label}With"] = stats["with"]
        macros[f"Ambiguity{label}Pct"] = f"{pct(stats['with'], stats['total']):.1f}"

    lines += [f"\\newcommand{{\\{k}}}{{{v}}}" for k, v in macros.items()]
    output_path.write_text("\n".join(lines) + "\n")
    print(f"\nWrote {len(macros)} macros to {output_path}")


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Ground-truth ambiguity (multi-map groups) across the human-authored corpus."
    )
    here = Path(__file__).resolve().parent
    root = repo_root(here)
    parser.add_argument(
        "--csv",
        type=Path,
        default=root / "research/data/quality/human_mapping_analysis.csv",
        help="defines which fixtures are in scope (Section 5's corpus state)",
    )
    parser.add_argument("--plots-dir", type=Path, default=root / "research/plots")
    args = parser.parse_args()

    names = fixture_names_in_scope(args.csv)
    groups_by_fixture = read_groups(root, names)
    missing = names - set(groups_by_fixture)
    if missing:
        print(
            f"warning: {len(missing)} fixture(s) in the CSV have no tracked mapping file: "
            f"{sorted(missing)[:5]}"
        )
    in_scope = set(groups_by_fixture)
    s = summarize(
        groups_by_fixture,
        paired_decisions(args.csv, in_scope),
        datasets_for(root, in_scope),
    )

    print(f"=== Ground-truth ambiguity, {s['scored']} fixtures in scope ===")
    print(f"Fixtures with >=1 multi-map group: {s['any_fixtures']} ({s['any_pct']:.1f}%)")
    print("\nBy repository list:")
    for label, stats in sorted(s["per_list"].items()):
        print(
            f"  {label:<10} {stats['with']:>3}/{stats['total']:<3} "
            f"({pct(stats['with'], stats['total']):5.1f}%)"
        )
    print(
        f"\nGroups: {s['groups']} total, {s['with_children']} with_children, "
        f"{s['op_identical']} identical / {s['op_match']} match-but-not-identical"
    )
    print(
        f"Size max(before, after): median {s['size_median']}, max {s['size_max']}; "
        f"{s['unequal_pct']:.1f}% have unequal sides"
    )
    print(
        "Most common shapes (before -> after): "
        + ", ".join(f"{b}->{a}: {n}" for (b, a), n in s["shapes"].most_common(6))
    )
    print(f"Most groups in one fixture: {s['max_groups_in_fixture']}")
    print(
        f"Pair-weighted: {s['ambiguous_pairs']} of {s['paired_decisions']} non-identical "
        f"pairings have no unique partner "
        f"({pct(s['ambiguous_pairs'], s['paired_decisions']):.1f}%)"
    )

    write_paper_fragment(s, args.plots_dir / "variables_ambiguity.tex")


if __name__ == "__main__":
    main()
