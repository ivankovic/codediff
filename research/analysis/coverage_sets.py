#!/usr/bin/env python3
#
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
"""Set operations over per-test line coverage.

Reads what `measure/per_test_coverage.py` wrote - one bitset of covered lines per test - and
combines them: unions, intersections, differences, a greedy set cover, the lines only one test
reaches, and groups of tests whose footprints are identical.

Every set is a Python int used as a bitset over `lines.tsv`'s order, so `&`, `|` and `& ~` are the
set operations and `.bit_count()` is the size. `--area` masks every set to one part of the tree
before anything else happens (the default, `engine`, is `src/diff*` and `src/code*`), and
`--group fixture` first merges a fixture's tests - mapping, painting, invariants and the rest - into
one set per fixture.

A selection is a regular expression over test names. Some useful ones:

  '::mapping$'                    every fixture's mapping test (the ones that run the diff)
  '^test::fixtures::handmade::'   one dataset
  '^(?!test::fixtures::)'         everything that is not a fixture test

Examples:

  uv run ./analysis/coverage_sets.py summary '::mapping$'
  uv run ./analysis/coverage_sets.py set '::handmade::' '::defects4j::' --group fixture
  uv run ./analysis/coverage_sets.py cover '::mapping$'
  uv run ./analysis/coverage_sets.py unique '::mapping$' --show 5
  uv run ./analysis/coverage_sets.py signatures '::mapping$'
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from collections import defaultdict
from pathlib import Path

import numpy as np

DEFAULT_DATA = Path(__file__).resolve().parent.parent / "data" / "coverage" / "per_test"
AREAS = ("engine", "harness", "other")


class Coverage:
    def __init__(self, data: Path, area: str, group: str):
        self.lines: list[tuple[str, str, int]] = []
        with open(data / "lines.tsv") as handle:
            next(handle)
            for row in handle:
                _, line_area, file, line = row.rstrip("\n").split("\t")
                self.lines.append((line_area, file, int(line)))
        self._area_masks: dict[str, int] = {}
        self.mask = 0
        for position, (line_area, _, _) in enumerate(self.lines):
            if area == "all" or line_area == area:
                self.mask |= 1 << position

        self.sets: dict[str, int] = {}
        self.skipped: dict[str, str] = {}
        with open(data / "records.jsonl") as handle:
            for raw in handle:
                if not raw.strip():
                    continue
                record = json.loads(raw)
                if record["status"] != "ok" or not record.get("bits"):
                    self.skipped[record["name"]] = record["status"]
                    continue
                bits = int.from_bytes((data / "bits" / record["bits"]).read_bytes(), "little")
                self.check(record, bits)
                self.sets[record["name"]] = bits & self.mask

        if group == "fixture":
            merged: dict[str, int] = defaultdict(int)
            for name, bits in self.sets.items():
                merged[fixture_of(name)] |= bits
            self.sets = dict(merged)

    def check(self, record: dict, bits: int) -> None:
        """The measurement's own per-area counts against the bitset read back: a mismatch means
        the bits files and lines.tsv come from different runs, and every answer would be wrong."""
        for line_area, expected in (record.get("covered") or {}).items():
            mask = self.area_mask(line_area)
            if (bits & mask).bit_count() != expected:
                sys.exit(
                    f"{record['name']}: {line_area} has {(bits & mask).bit_count()} bits set but"
                    f" the run recorded {expected} - lines.tsv and bits/ do not belong together"
                )

    def area_mask(self, area: str) -> int:
        if area not in self._area_masks:
            mask = 0
            for position, (line_area, _, _) in enumerate(self.lines):
                if line_area == area:
                    mask |= 1 << position
            self._area_masks[area] = mask
        return self._area_masks[area]

    def reach_counts(self, sets) -> np.ndarray:
        """How many of `sets` reach each line, as one array over lines.tsv's order. numpy rather
        than a per-bit loop: a few thousand sets over ~32k lines is ~100M bit tests in Python."""
        width = (len(self.lines) + 7) // 8
        counts = np.zeros(len(self.lines), dtype=np.int32)
        for bits in sets:
            row = np.frombuffer(bits.to_bytes(width, "little"), dtype=np.uint8)
            counts += np.unpackbits(row, bitorder="little")[: len(self.lines)]
        return counts

    @staticmethod
    def bits_where(flags: np.ndarray) -> int:
        """The bitset of the positions where `flags` is true - the inverse of `reach_counts`."""
        packed = np.packbits(flags.astype(np.uint8), bitorder="little")
        return int.from_bytes(packed.tobytes(), "little")

    def select(self, pattern: str | None) -> dict[str, int]:
        if not pattern:
            return dict(self.sets)
        compiled = re.compile(pattern)
        chosen = {name: bits for name, bits in self.sets.items() if compiled.search(name)}
        if not chosen:
            sys.exit(f"no measured test or fixture matches {pattern!r}")
        return chosen

    def describe(self, bits: int, limit: int) -> list[str]:
        """`file:first-last` ranges, one per run of consecutive covered lines in a file."""
        by_file: dict[str, list[int]] = defaultdict(list)
        position = 0
        while bits:
            if bits & 1:
                _, file, line = self.lines[position]
                by_file[file].append(line)
            bits >>= 1
            position += 1
        ranges = []
        for file in sorted(by_file):
            numbers = by_file[file]
            start = previous = numbers[0]
            for number in numbers[1:] + [None]:
                if number is not None and number == previous + 1:
                    previous = number
                    continue
                ranges.append(f"{file}:{start}" + (f"-{previous}" if previous != start else ""))
                if number is not None:
                    start = previous = number
        if limit and len(ranges) > limit:
            return ranges[:limit] + [f"... {len(ranges) - limit} more ranges"]
        return ranges


def fixture_of(name: str) -> str:
    """`test::fixtures::<dataset>::<stub>` for a fixture test; the test itself otherwise."""
    parts = name.split("::")
    if len(parts) >= 5 and parts[0] == "test" and parts[1] == "fixtures":
        return "::".join(parts[:4])
    return name


def union(sets) -> int:
    result = 0
    for bits in sets:
        result |= bits
    return result


def intersection(sets) -> int:
    sets = list(sets)
    if not sets:
        return 0
    result = sets[0]
    for bits in sets[1:]:
        result &= bits
    return result


def percentile(values: list[int], fraction: float) -> int:
    ordered = sorted(values)
    return ordered[min(len(ordered) - 1, int(fraction * len(ordered)))]


def show(coverage: Coverage, label: str, bits: int, limit: int) -> None:
    print(f"  {label}: {bits.bit_count()} lines")
    if limit:
        for entry in coverage.describe(bits, limit):
            print(f"      {entry}")


def cmd_summary(coverage: Coverage, args) -> None:
    sets = coverage.select(args.select)
    total = coverage.mask.bit_count()
    every = union(sets.values())
    common = intersection(sets.values())
    sizes = [bits.bit_count() for bits in sets.values()]
    print(f"{len(sets)} sets, over {total} instrumented lines in area '{args.area}'")
    print(
        f"  union (reached by at least one): {every.bit_count():>6}  "
        f"({100 * every.bit_count() / max(total, 1):.1f}% of the area)"
    )
    print(f"  intersection (reached by all):   {common.bit_count():>6}")
    print(
        f"  per set: min {min(sizes)}, p10 {percentile(sizes, 0.1)}, median "
        f"{percentile(sizes, 0.5)}, p90 {percentile(sizes, 0.9)}, max {max(sizes)}"
    )
    reach = coverage.reach_counts(sets.values())
    print("  how many sets reach each line of the union:")
    buckets = [(1, 1), (2, 5), (6, 20), (21, 100), (101, 500), (501, 10**9)]
    for low, high in buckets:
        n = int(((reach >= low) & (reach <= high)).sum())
        label = f"{low}" if low == high else (f"{low}+" if high >= 10**9 else f"{low}-{high}")
        print(f"      {label:>7} sets: {n:>6} lines")
    if coverage.skipped:
        print(
            f"  not included ({len(coverage.skipped)} tests): "
            + ", ".join(
                f"{s} x{list(coverage.skipped.values()).count(s)}"
                for s in sorted(set(coverage.skipped.values()))
            )
        )


def cmd_set(coverage: Coverage, args) -> None:
    a = coverage.select(args.a)
    print(f"A = {args.a!r}: {len(a)} sets")
    union_a, common_a = union(a.values()), intersection(a.values())
    show(coverage, "union of A", union_a, 0)
    show(coverage, "intersection of A", common_a, args.show if not args.b else 0)
    if not args.b:
        return
    b = coverage.select(args.b)
    print(f"B = {args.b!r}: {len(b)} sets")
    union_b, common_b = union(b.values()), intersection(b.values())
    show(coverage, "union of B", union_b, 0)
    show(coverage, "intersection of B", common_b, 0)
    show(coverage, "A and B (both unions)", union_a & union_b, 0)
    show(coverage, "only A (in A's union, no B reaches it)", union_a & ~union_b, args.show)
    show(coverage, "only B (in B's union, no A reaches it)", union_b & ~union_a, args.show)


def cmd_cover(coverage: Coverage, args) -> None:
    """Greedy set cover: repeatedly take the set that adds the most lines not yet covered. Not
    minimal - that problem is NP-hard - but within a logarithmic factor of it, and it is what any
    coverage-driven selection would do in practice."""
    remaining = coverage.select(args.select)
    goal = union(remaining.values())
    target = goal.bit_count()
    covered = 0
    chosen: list[tuple[str, int]] = []
    marks = {90: None, 95: None, 99: None, 100: None}
    while covered != goal:
        name, bits = max(remaining.items(), key=lambda item: (item[1] & ~covered).bit_count())
        gain = (bits & ~covered).bit_count()
        covered |= bits
        chosen.append((name, gain))
        del remaining[name]
        for percent in marks:
            if marks[percent] is None and covered.bit_count() * 100 >= percent * target:
                marks[percent] = len(chosen)
    total_sets = len(chosen) + len(remaining)
    print(f"{total_sets} sets reach {target} lines between them. Greedy cover:")
    for percent, count in marks.items():
        print(f"  {percent:>3}% of those lines: {count:>5} sets ({100 * count / total_sets:.1f}%)")
    print(f"\nFirst {min(args.show, len(chosen))} picks (lines each adds):")
    for name, gain in chosen[: args.show]:
        print(f"  {gain:>6}  {name}")


def cmd_unique(coverage: Coverage, args) -> None:
    sets = coverage.select(args.select)
    # A line is unique to a set exactly when one set reaches it, so one count over all sets
    # answers every set at once rather than a union of all the others per set.
    reached_once = coverage.bits_where(coverage.reach_counts(sets.values()) == 1)
    rows = []
    for name, bits in sets.items():
        only = bits & reached_once
        if only:
            rows.append((only.bit_count(), name, only))
    rows.sort(reverse=True)
    print(
        f"{len(rows)} of {len(sets)} sets reach at least one line no other set in the selection "
        f"reaches ({sum(r[0] for r in rows)} such lines in all)"
    )
    for count, name, only in rows[: args.top]:
        print(f"  {count:>5}  {name}")
        if args.show:
            for entry in coverage.describe(only, args.show):
                print(f"           {entry}")


def cmd_signatures(coverage: Coverage, args) -> None:
    sets = coverage.select(args.select)
    groups: dict[int, list[str]] = defaultdict(list)
    for name, bits in sets.items():
        groups[bits].append(name)
    ordered = sorted(groups.values(), key=len, reverse=True)
    print(f"{len(sets)} sets have {len(groups)} distinct footprints")
    sizes = [len(members) for members in ordered]
    print(f"  singletons: {sizes.count(1)}, largest group: {sizes[0]}")
    for members in ordered[: args.top]:
        if len(members) < 2:
            break
        print(
            f"  {len(members):>4} identical: {', '.join(members[:3])}"
            + (f" ... +{len(members) - 3}" if len(members) > 3 else "")
        )


def main() -> int:
    parser = argparse.ArgumentParser(
        description=__doc__.split("\n\n")[0],
        epilog=__doc__.split("\n\n", 1)[1],
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument("--data", type=Path, default=DEFAULT_DATA)
    parser.add_argument("--area", default="engine", choices=[*AREAS, "all"])
    parser.add_argument("--group", default="test", choices=["test", "fixture"])
    sub = parser.add_subparsers(dest="command", required=True)

    p = sub.add_parser("summary", help="union, intersection and how widely each line is reached")
    p.add_argument("select", nargs="?")
    p = sub.add_parser("set", help="union/intersection of A; with B, the differences too")
    p.add_argument("a")
    p.add_argument("b", nargs="?")
    p.add_argument("--show", type=int, default=0, help="list up to N line ranges")
    p = sub.add_parser("cover", help="greedy set cover of the selection's union")
    p.add_argument("select", nargs="?")
    p.add_argument("--show", type=int, default=15, help="list the first N picks")
    p = sub.add_parser("unique", help="sets that alone reach some line")
    p.add_argument("select", nargs="?")
    p.add_argument("--top", type=int, default=20)
    p.add_argument("--show", type=int, default=0, help="list up to N line ranges per set")
    p = sub.add_parser("signatures", help="groups of sets with identical footprints")
    p.add_argument("select", nargs="?")
    p.add_argument("--top", type=int, default=15)

    args = parser.parse_args()
    coverage = Coverage(args.data, args.area, args.group)
    {
        "summary": cmd_summary,
        "set": cmd_set,
        "cover": cmd_cover,
        "unique": cmd_unique,
        "signatures": cmd_signatures,
    }[args.command](coverage, args)
    return 0


if __name__ == "__main__":
    sys.exit(main())
