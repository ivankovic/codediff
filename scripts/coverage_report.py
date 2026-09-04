#!/usr/bin/env python3
"""Turn `cargo llvm-cov --json --summary-only` into a per-area table.

llvm-cov's own table is one row per file, 675 of them here, which buries the thing worth knowing:
the diff engine and the dev tools are held to very different standards on purpose. `src/bin/`
holds samplers, benchmark harnesses and human_solver - tools whose value is what they let a human
do, several of which exist to be run once and read. Averaging them together with `src/diff/`
produces a number that means nothing about either.

Reads the JSON on stdin so it composes with whatever llvm-cov invocation the caller wants.
"""

import json
import sys

# Longest prefix wins, so `src/bin/human_solver/` can be split out of `src/bin/` if that is ever
# wanted. Order here is display order.
AREAS = [
    ("src/diff/", "diff/ - the engine"),
    ("src/code/", "code/ - parsing, metadata"),
    ("src/tui/", "tui/ - viewer, headless"),
    ("src/stats/", "stats/ - sampling, git"),
    ("src/test/", "test/ - fixture helpers"),
    ("src/bin/", "bin/ - dev tools"),
]


def area_of(path: str) -> str:
    best = ""
    label = "other"
    for prefix, name in AREAS:
        if prefix in path and len(prefix) > len(best):
            best, label = prefix, name
    return label


def main() -> int:
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
    for _, label in AREAS:
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

    worst.sort(reverse=True)
    print("\nMost uncovered lines")
    for missed, filename, percent in worst[:10]:
        short = filename.split("/src/", 1)[-1]
        print(f"  {missed:>5} missed  {percent:5.1f}%  {short}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
