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

=== Why this script splits the corpus by annotation era ===

`groups` was added to the annotation tool partway through corpus construction (commit a8109f6,
2026-08-09). A fixture annotated before that date could not record an ambiguity even where one
existed, and most were never revisited. Reporting one corpus-wide rate would therefore measure
*annotation practice*, not the phenomenon - the difference between a prevalence claim and an
artifact of tooling history. So every fixture is classified into one of three eras, from git
history of its own `human_mapping.json`:

* `pre`       - last touched before the facility existed. Structurally cannot contain a group.
* `fresh`     - the file first appears on or after that date, i.e. annotated from the start by
                someone who had multi-mapping available. This is the only unbiased population.
* `revisited` - existed before the facility, edited after it. Selection-biased in the obvious
                direction (a fixture gets reopened *because* something needed fixing there), so
                it is reported separately and excluded from the headline rate.

The paper quotes the `fresh` rate as the estimate and the corpus-wide rate as a floor. Even the
`fresh` rate is a lower bound: an annotator can miss an ambiguity that is really there.

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

# Commit a8109f6, "Add multi-map groups to human_mapping ground truth". Every mapping file whose
# last commit predates this could not have recorded a group, whatever its content. Kept as a date
# rather than a hash so the comparison against `git log --date=short` output is a plain string
# compare, and so a reader can check it without resolving a hash.
GROUPS_LANDED = "2026-08-09"

# Path prefix every fixture directory lives under, relative to the repository root.
DIFFS_ROOT = "src/test/data/diffs"


def repo_root(here: Path) -> Path:
    return here.parent.parent


def fixture_names_in_scope(csv_path: Path) -> set[str]:
    """The fixture set Section 5 is measured against - see this script's "Corpus state" note."""
    with open(csv_path, newline="") as f:
        return {r["name"] for r in csv.DictReader(f) if r["has_mapping"] == "true"}


def _log_dates(root: Path) -> tuple[dict[str, str], dict[str, str]]:
    """(newest, oldest) commit date per repo-relative path, from one `git log --name-only` walk.

    Deliberately not `--diff-filter=A`: that walk reports a path only where it was *added*, and a
    file moved into its current directory appears as a rename instead, so 232 of this corpus's 471
    mapping files are absent from it (the fixtures predate the small/full/handmade split). Taking
    the oldest date the path itself appears under is rename-robust and classifies the corpus
    identically, without a lookup that silently returns nothing for half the files.
    """
    out = subprocess.run(
        ["git", "log", "--format=@@%ad", "--date=short", "--name-only", "--", DIFFS_ROOT],
        capture_output=True,
        text=True,
        cwd=root,
        check=True,
    ).stdout
    newest: dict[str, str] = {}
    oldest: dict[str, str] = {}
    date = ""
    for line in out.splitlines():
        if line.startswith("@@"):
            date = line[2:]
        elif line.strip():
            newest.setdefault(line, date)
            oldest[line] = date  # the walk is newest-first, so the last write is the oldest commit
    return newest, oldest


def derive_eras(root: Path, names: set[str]) -> dict[str, str]:
    """Fixture name -> "pre" | "fresh" | "revisited", from git history. See the doc comment.

    Raises on a tracked mapping file that `git log` did not date. That combination should be
    impossible, and swallowing it would silently move a fixture into `pre` - i.e. into the
    denominator of the paper's headline rate - on a data gap rather than on a measurement.
    """
    tracked = subprocess.run(
        ["git", "ls-files", DIFFS_ROOT],
        capture_output=True,
        text=True,
        cwd=root,
        check=True,
    ).stdout.split()
    mappings = [p for p in tracked if p.endswith("human_mapping.json")]
    newest, oldest = _log_dates(root)

    eras = {}
    for path in mappings:
        name = path.split("/")[-2]
        if name not in names:
            continue
        if path not in newest:
            raise RuntimeError(
                f"{path} is tracked but git log did not date it; refusing to classify its "
                f"annotation era by default (see derive_eras)"
            )
        if newest[path] < GROUPS_LANDED:
            eras[name] = "pre"
        elif oldest[path] >= GROUPS_LANDED:
            eras[name] = "fresh"
        else:
            eras[name] = "revisited"
    return eras


def load_or_record_eras(root: Path, names: set[str], cache_path: Path, refresh: bool) -> dict:
    """The era classification, read from its committed record and extended only for new fixtures.

    Deriving this from `git log` on every run would make the paper's headline rate a function of
    mutable history: one formatting sweep over src/test/data/diffs/ moves every `pre` fixture into
    `revisited` and changes the AmbiguityFreshPct macro with no corpus change at all. So the derived
    classification is committed to data/quality/ambiguity_eras.csv and treated as the record;
    fixtures absent from it (i.e. added since) are derived and appended, and `--refresh-eras`
    re-derives everything from scratch for the one case where that is actually wanted.
    """
    recorded: dict[str, str] = {}
    if cache_path.exists() and not refresh:
        with open(cache_path, newline="") as f:
            recorded = {r["name"]: r["era"] for r in csv.DictReader(f)}

    missing = sorted(names - set(recorded))
    if missing or refresh:
        derived = derive_eras(root, names if refresh else set(missing))
        if missing and not refresh:
            print(
                f"note: {len(missing)} fixture(s) not in {cache_path.name}; classifying from "
                f"git history and appending"
            )
        recorded.update(derived)
        rows = sorted((n, e) for n, e in recorded.items() if n in names)
        with open(cache_path, "w", newline="") as f:
            w = csv.writer(f)
            w.writerow(["name", "era"])
            w.writerows(rows)
    return {n: e for n, e in recorded.items() if n in names}


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


def summarize(groups_by_fixture: dict[str, list[dict]], eras: dict[str, str], paired: int) -> dict:
    """Every number the paper's RQ3 quotes, in one dict."""
    names = sorted(groups_by_fixture)
    with_groups = [n for n in names if groups_by_fixture[n]]

    by_era = {
        era: [n for n in names if eras.get(n) == era] for era in ("pre", "fresh", "revisited")
    }
    era_rates = {
        era: (len([n for n in members if groups_by_fixture[n]]), len(members))
        for era, members in by_era.items()
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
        "era_rates": era_rates,
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
    fresh_with, fresh_total = s["era_rates"]["fresh"]
    rev_with, rev_total = s["era_rates"]["revisited"]
    pre_with, pre_total = s["era_rates"]["pre"]

    macros = {
        # Corpus-wide, i.e. the floor: includes fixtures annotated before groups existed.
        "AmbiguityScored": s["scored"],
        "AmbiguityAnyFixtures": s["any_fixtures"],
        "AmbiguityAnyPct": f"{s['any_pct']:.1f}",
        # The three annotation eras (see this script's doc comment). `fresh` carries the estimate.
        "AmbiguityFreshFixtures": fresh_with,
        "AmbiguityFreshScored": fresh_total,
        "AmbiguityFreshPct": f"{pct(fresh_with, fresh_total):.1f}",
        "AmbiguityPreFixtures": pre_total,
        "AmbiguityPreWith": pre_with,
        "AmbiguityPrePct": f"{pct(pre_with, pre_total):.1f}",
        "AmbiguityRevisitedFixtures": rev_total,
        "AmbiguityRevisitedWith": rev_with,
        "AmbiguityRevisitedPct": f"{pct(rev_with, rev_total):.1f}",
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
    parser.add_argument(
        "--eras",
        type=Path,
        default=root / "research/data/quality/ambiguity_eras.csv",
        help="committed record of each fixture's annotation era (see load_or_record_eras)",
    )
    parser.add_argument(
        "--refresh-eras",
        action="store_true",
        help="re-derive every fixture's era from git history, overwriting the record. Changes the "
        "paper's headline rate whenever src/test/data/diffs/ has been touched since - only "
        "use this deliberately.",
    )
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
    eras = load_or_record_eras(root, in_scope, args.eras, args.refresh_eras)
    s = summarize(groups_by_fixture, eras, paired_decisions(args.csv, in_scope))

    print(f"=== Ground-truth ambiguity, {s['scored']} fixtures in scope ===")
    print(
        f"Fixtures with >=1 multi-map group: {s['any_fixtures']} ({s['any_pct']:.1f}%) "
        f"- corpus-wide, a floor"
    )
    print(f"\nBy annotation era (groups landed {GROUPS_LANDED}):")
    for era in ("pre", "fresh", "revisited"):
        with_g, total = s["era_rates"][era]
        print(f"  {era:<10} {with_g:>3}/{total:<3} ({pct(with_g, total):5.1f}%)")
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
