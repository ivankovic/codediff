#!/usr/bin/env python3
# This file is part of the CodeDiff code diffing tool.
#
# Copyright (C) 2026 Marko Ivankovic
#
# This program is free software: you can redistribute it and/or modify
# it under the terms of the GNU Affero General Public License as published
# by the Free Software Foundation, either version 3 of the License, or
# (at your option) any later version.
#
# This program is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
# GNU Affero General Public License for more details.
#
# You should have received a copy of the GNU Affero General Public License
# along with this program. If not, see <https://www.gnu.org/licenses/>.

"""Merge an R48 run's shard parts and killed records into one CSV, and summarise it.

`measure/overnight_benchmarks.sh` measures the Full corpus in shards, each of which may take
several attempts (a shard resumes after its process is killed), so a shard's measurements are
spread over `robustness_shard_<i>.part*.csv` files plus a `robustness_shard_<i>.killed.csv`
naming the pair each attempt died on and the exit status it died with. This merges them into
`robustness_full.csv` with one row per pair - the last row wins where a pair was measured more
than once, and a killed record is kept only for a pair with no measured row - and prints the
status counts and the largest pair.

Exit statuses mean different things and the summary keeps them apart: 137 is the cgroup memory
cap (the pair needs more than SHARD_MEMORY_MAX), 134 is an abort inside the process, 143 is
SIGTERM (an operator restarting a shard, not a finding). `measure/r48_retry_killed.sh`
re-measures the 134s and the 143s and gives the 137s a bigger cap, one at a time, before this is
final.
"""

import argparse
import collections
import csv
import glob
import json
import os
from pathlib import Path

KILL_MEANING = {
    "137": "memory cap (SIGKILL)",
    "134": "abort (SIGABRT)",
    "143": "restarted by operator (SIGTERM)",
}


def key(row):
    return (row["repository"], row["commit"], row["path"])


def merge(out: Path, shards: int, merged: Path) -> dict:
    measured: dict = {}
    header = None
    for i in range(shards):
        for p in sorted(glob.glob(f"{out}/robustness_shard_{i}.part*.csv")):
            with open(p, newline="") as f:
                reader = csv.DictReader(f)
                if reader.fieldnames and header is None:
                    header = reader.fieldnames
                for row in reader:
                    if len(row) == len(header) and None not in row.values():
                        measured[key(row)] = row
    if header is None:
        raise SystemExit(f"no shard parts under {out}")
    killed: dict = {}
    for i in range(shards):
        p = f"{out}/robustness_shard_{i}.killed.csv"
        if not os.path.exists(p):
            continue
        with open(p, newline="") as f:
            for row in csv.DictReader(f):
                if key(row) not in measured:
                    killed[key(row)] = row
    with open(merged, "w", newline="") as fo:
        w = csv.DictWriter(fo, fieldnames=header + ["exit_status"])
        w.writeheader()
        for row in measured.values():
            w.writerow({**row, "exit_status": ""})
        for row in killed.values():
            w.writerow(
                {k: row.get(k, "") for k in header} | {"exit_status": row.get("exit_status", "")}
            )
    return {"measured": len(measured), "killed": len(killed)}


def percentile(values: list, q: float):
    """Nearest rank on a sorted list, the definition every other report here uses."""
    if not values:
        return None
    index = min(len(values) - 1, max(0, round(q / 100 * (len(values) - 1))))
    return values[index]


def summarise(merged: Path) -> dict:
    """The aggregates the paper quotes, as one JSON-able dict: what the run is (pairs, files,
    languages), how it ended per status, and the completed pairs' size, time and memory
    distributions. Also the committed record of the run, since the merged CSV itself (hundreds
    of thousands of rows) is not."""
    counts: collections.Counter = collections.Counter()
    kills: collections.Counter = collections.Counter()
    killed_files: set = set()
    ok_files: dict = {}  # (repository, path) -> max peak bytes over its completed pairs
    languages: set = set()
    biggest = (0, None)
    slowest = (0.0, None)
    elapsed: list = []
    peak: list = []
    nodes_ok: list = []
    max_side = (0, None)
    with open(merged, newline="") as f:
        for r in csv.DictReader(f):
            counts[r["status"]] += 1
            languages.add(r["language"])
            nodes = int(r.get("ast_nodes_before") or 0) + int(r.get("ast_nodes_after") or 0)
            if r["status"] == "killed":
                kills[KILL_MEANING.get(r["exit_status"], f"exit {r['exit_status']}")] += 1
                killed_files.add((r["repository"], r["path"]))
            if nodes > biggest[0]:
                biggest = (nodes, f"{r['repository']} {r['commit'][:10]} {r['path']}")
            if r["status"] == "ok":
                side = max(int(r["ast_nodes_before"] or 0), int(r["ast_nodes_after"] or 0))
                if side > max_side[0]:
                    max_side = (side, f"{r['repository']} {r['commit'][:10]} {r['path']}")
                ms = float(r["elapsed_ms"] or 0)
                elapsed.append(ms)
                peak.append(int(r["peak_memory_bytes"] or 0))
                fk = (r["repository"], r["path"])
                ok_files[fk] = max(ok_files.get(fk, 0), int(r["peak_memory_bytes"] or 0))
                nodes_ok.append(nodes)
                if ms > slowest[0]:
                    slowest = (ms, f"{r['repository']} {r['commit'][:10]} {r['path']}")
    elapsed.sort()
    peak.sort()
    nodes_ok.sort()
    # A memory-killed file whose representative commit then completed under the retry pass's
    # larger cap has both a killed row (its other commits) and an ok row; one that died again
    # under that cap has only killed rows.
    killed_then_completed = {f: ok_files[f] for f in killed_files if f in ok_files}
    summary = {
        "pairs": sum(counts.values()),
        "languages": len(languages),
        "status_counts": dict(counts),
        "killed_by": dict(kills),
        "killed_distinct_files": len(killed_files),
        "killed_files_completed_under_big_cap": len(killed_then_completed),
        "killed_files_completed_under_big_cap_max_peak_bytes": max(
            killed_then_completed.values(), default=0
        ),
        "largest_pair_combined_nodes": biggest[0],
        "largest_pair": biggest[1],
        "completed": {
            "count": len(elapsed),
            "max_combined_nodes": nodes_ok[-1] if nodes_ok else 0,
            "max_side_nodes": max_side[0],
            "max_side_pair": max_side[1],
            "elapsed_ms": {
                "p50": percentile(elapsed, 50),
                "p99": percentile(elapsed, 99),
                "p999": percentile(elapsed, 99.9),
                "max": elapsed[-1] if elapsed else None,
            },
            "peak_memory_bytes": {
                "p50": percentile(peak, 50),
                "p99": percentile(peak, 99),
                "max": peak[-1] if peak else None,
            },
            "slowest_pair": slowest[1],
        },
    }
    print("status counts:", dict(counts))
    if kills:
        print("killed by:", dict(kills), "over", len(killed_files), "distinct files")
    print("largest pair by combined AST nodes:", biggest)
    print("slowest completed pair, ms:", slowest)
    return summary


def write_exceptions(merged: Path, out: Path) -> int:
    """Every pair that did not complete, as a small CSV that can be committed: the finding is
    in these rows, and the hundreds of thousands of completed ones are not."""
    with open(merged, newline="") as f, open(out, "w", newline="") as fo:
        reader = csv.DictReader(f)
        w = csv.DictWriter(fo, fieldnames=reader.fieldnames)
        w.writeheader()
        n = 0
        for r in reader:
            if r["status"] != "ok":
                w.writerow(r)
                n += 1
    return n


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, default=Path("/var/tmp/research/full/robustness"))
    parser.add_argument("--shards", type=int, default=8)
    parser.add_argument(
        "--summary-json",
        type=Path,
        help="Write the aggregates here (the committed record; paper_variables.py reads it).",
    )
    parser.add_argument(
        "--exceptions-csv",
        type=Path,
        help="Write every pair that did not complete here (the committed finding).",
    )
    args = parser.parse_args()
    merged = args.out / "robustness_full.csv"
    counts = merge(args.out, args.shards, merged)
    print(f"{counts['measured']} measured and {counts['killed']} killed pairs merged into {merged}")
    summary = summarise(merged)
    if args.summary_json:
        args.summary_json.parent.mkdir(parents=True, exist_ok=True)
        args.summary_json.write_text(json.dumps(summary, indent=2) + "\n")
        print(f"summary written to {args.summary_json}")
    if args.exceptions_csv:
        n = write_exceptions(merged, args.exceptions_csv)
        print(f"{n} exceptions written to {args.exceptions_csv}")


if __name__ == "__main__":
    main()
