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
#  MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
#  GNU Affero General Public License for more details.
#
#  You should have received a copy of the GNU Affero General Public License
#  along with this program. If not, see <https://www.gnu.org/licenses/>.

"""Turn `cargo llvm-cov --json --summary-only` into a per-area table.

llvm-cov's own table is one row per file, 675 of them here, which buries the thing worth knowing:
the diff engine and the dev tools are held to very different standards on purpose. `src/bin/`
holds samplers, benchmark harnesses and human_solver - tools whose value is what they let a human
do, several of which exist to be run once and read. Averaging them together with `src/diff/`
produces a number that means nothing about either.

Reads the JSON on stdin so it composes with whatever llvm-cov invocation the caller wants.

`--badge` additionally writes a shields.io endpoint file, which is what the README's badge reads.
Both numbers go on it: a badge showing only the 91% would be quietly choosing the flattering half,
and one showing only the 79% would describe the sampler harnesses rather than the diff engine.
"""

import argparse
import json
import re
import sys
from pathlib import Path

# Longest prefix wins, so `src/bin/human_solver/` can be split out of `src/bin/` if that is ever
# wanted. Order here is display order.
#
# Each module is two prefixes, the directory *and* the module file beside it: `src/diff/` is the
# engine's submodules and `src/diff.rs` is the engine's own root, 581 lines of it. Matching only
# the directory would send every one of those roots - diff, code, test, stats, tui - to `other`,
# which no row prints and no total but EVERYTHING counts, and the product figure would silently
# leave them out.
AREAS = [
    (("src/diff/", "src/diff.rs"), "diff/ - the engine"),
    (("src/code/", "src/code.rs"), "code/ - parsing, metadata"),
    (("src/tui/", "src/tui.rs"), "tui/ - viewer, headless"),
    (("src/stats/", "src/stats.rs"), "stats/ - sampling, git"),
    (("src/showcase/", "src/showcase.rs"), "showcase/ - showcase data"),
    (("src/test/", "src/test.rs"), "test/ - fixture helpers"),
    (("src/bin/",), "bin/ - dev tools"),
]

# Files sitting directly in `src/` that are nobody's module root: `main.rs`,
# `review.rs`, the git/jj integrations. Product code, so it belongs in the product total, but it
# is not part of any module above and reads better as its own line than folded into one.
TOP_LEVEL = "src/ - entry points, integrations"


def area_of(path: str) -> str:
    best = ""
    label = ""
    for prefixes, name in AREAS:
        for prefix in prefixes:
            if prefix in path and len(prefix) > len(best):
                best, label = prefix, name
    if label:
        return label
    # `src/<something>.rs` with nothing between: a top-level file, not an unclassified one.
    if re.search(r"(^|/)src/[^/]+\.rs$", path):
        return TOP_LEVEL
    return "other"


def badge_color(percent: float) -> str:
    """shields.io's own palette, at the thresholds it uses for coverage by convention."""
    for floor, color in (
        (90, "brightgreen"),
        (80, "green"),
        (70, "yellowgreen"),
        (60, "yellow"),
    ):
        if percent >= floor:
            return color
    return "orange"


def write_badge(path: Path, product: list[int], everything: list[int]) -> None:
    lib = 100.0 * product[0] / product[1] if product[1] else 0.0
    total = 100.0 * everything[0] / everything[1] if everything[1] else 0.0
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        json.dumps(
            {
                "schemaVersion": 1,
                "label": "coverage",
                "message": f"{lib:.0f}% lib · {total:.0f}% all",
                "color": badge_color(lib),
            },
            indent=2,
        )
        + "\n"
    )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument(
        "--badge",
        type=Path,
        metavar="PATH",
        help="also write a shields.io endpoint JSON here, for the README badge",
    )
    args = parser.parse_args()

    report = json.load(sys.stdin)
    files = report["data"][0]["files"]

    totals: dict[str, list[int]] = {}
    worst: list[tuple[int, str, float]] = []
    for entry in files:
        lines = entry["summary"]["lines"]
        covered, count = lines["covered"], lines["count"]
        if count == 0:
            continue
        bucket = totals.setdefault(area_of(entry["filename"]), [0, 0])
        bucket[0] += covered
        bucket[1] += count
        worst.append((count - covered, entry["filename"], lines["percent"]))

    def row(label: str, covered: int, count: int) -> str:
        percent = 100.0 * covered / count if count else 0.0
        return f"  {label:<26} {covered:>6}/{count:<6} lines  {percent:5.1f}%"

    print("Line coverage by area")
    product = [0, 0]
    # `other` is `area_of`'s fallback and is deliberately printed last rather than skipped: a
    # source directory nobody added to AREAS used to vanish from every row while still counting
    # toward EVERYTHING, which is how a whole module once stayed invisible. A row that reads
    # "other" is a prompt to add the prefix above.
    for label in [name for _, name in AREAS] + [TOP_LEVEL, "other"]:
        if label not in totals:
            continue
        covered, count = totals[label]
        print(row(label, covered, count))
        if not label.startswith("bin/"):
            product[0] += covered
            product[1] += count
    print()
    print(row("PRODUCT (everything but bin/)", *product))
    everything = [
        sum(v[0] for v in totals.values()),
        sum(v[1] for v in totals.values()),
    ]
    print(row("EVERYTHING", *everything))

    if args.badge:
        write_badge(args.badge, product, everything)
        print(f"\nBadge written to {args.badge}")

    worst.sort(reverse=True)
    print("\nMost uncovered lines")
    for missed, filename, percent in worst[:10]:
        short = filename.split("/src/", 1)[-1]
        print(f"  {missed:>5} missed  {percent:5.1f}%  {short}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
