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
"""Line coverage of every test in the library's test binary, each measured alone.

`make coverage` answers "what does the suite cover". This answers "what does *each test* cover",
so the results can be combined with set operations: which lines every fixture reaches, which lines
exactly one test reaches, which tests have identical footprints. `analysis/coverage_sets.py` does
the combining; this only measures.

One test per process, by construction. The library's test binary runs a single test with
`--exact <name>`, and `LLVM_PROFILE_FILE` names that process's profile, so each profile holds one
test's counters and nothing else. Running the whole suite once and splitting it afterwards is not
possible: a profile records counts, not which test incremented them.

Output, under `--out`:

  lines.tsv      index, area, file, line - every instrumented line, the bit order of every set
  records.jsonl  one line per test: name, status, seconds, lines covered per area, bits file
  bits/<key>     that test's covered lines as a little-endian bitset over lines.tsv's order

Lines in `src/test/fixtures/` are left out entirely. Each of those 1220 files is one fixture's test
stub, reached by that fixture's tests and no others, so keeping them would give every fixture test
lines "only it covers" that say nothing about the engine.

Resumable: a test already in records.jsonl is skipped, whatever its status, so a test that was
killed stays recorded as killed rather than being retried forever. Delete its line to retry it.

Run it as its own systemd unit, with every test in its own memory-capped scope - see
`research/measure/overnight_benchmarks.sh` for why (an OOM inside the shell's scope took down the
whole session on 2026-09-19):

  systemd-run --user --unit codediff-per-test-coverage --collect --same-dir \\
    --setenv=PATH="$PATH" ./measure/per_test_coverage.py --binary <instrumented test binary> \\
    --out data/coverage/per_test

  systemctl --user stop codediff-per-test-coverage codediff-per-test-coverage.slice

The instrumented binary comes from `cargo llvm-cov show-env`; `make measure-per-test-coverage` in
research/Makefile builds it and starts the unit.
"""

from __future__ import annotations

import argparse
import concurrent.futures
import hashlib
import json
import os
import re
import subprocess
import sys
import threading
import time
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
SLICE = "codediff-per-test-coverage.slice"

# Everything outside this repository's own source, plus the fixture stubs (see the docstring).
IGNORE = r"(/\.cargo/|/rustc/|/target/|/src/test/fixtures/)"


def llvm_tool(name: str) -> str:
    sysroot = subprocess.run(
        ["rustc", "--print", "sysroot"], capture_output=True, text=True, check=True
    ).stdout.strip()
    host = next(
        line.split(":", 1)[1].strip()
        for line in subprocess.run(
            ["rustc", "-vV"], capture_output=True, text=True, check=True
        ).stdout.splitlines()
        if line.startswith("host:")
    )
    path = Path(sysroot) / "lib" / "rustlib" / host / "bin" / name
    if not path.exists():
        sys.exit(f"{path} is missing - install it with `rustup component add llvm-tools`")
    return str(path)


def area_of(path: str) -> str:
    """Which part of the tree a source file is. Coarse on purpose: the analysis filters by it."""
    if re.match(r"src/(diff|code)(\.rs|/)", path):
        return "engine"
    if re.match(r"src/test(\.rs|/)", path):
        return "harness"
    return "other"


def list_tests(binary: str, work: Path) -> list[str]:
    # An instrumented binary writes a profile on every exit, and with no LLVM_PROFILE_FILE it
    # writes `default_*.profraw` into the working directory - the repository root, here.
    profile = work / "list.profraw"
    out = subprocess.run(
        [binary, "--list"],
        env={**os.environ, "LLVM_PROFILE_FILE": str(profile)},
        capture_output=True,
        text=True,
        check=True,
        cwd=REPO,
    ).stdout
    profile.unlink(missing_ok=True)
    return [line[: -len(": test")] for line in out.splitlines() if line.endswith(": test")]


def covered_lines(binary: str, profraw: Path) -> list[tuple[str, int, int]]:
    """(file, line, count) for every instrumented line, from one profile."""
    profdata = profraw.with_suffix(".profdata")
    subprocess.run(
        [TOOLS["profdata"], "merge", "-sparse", str(profraw), "-o", str(profdata)],
        check=True,
        capture_output=True,
    )
    lcov = subprocess.run(
        [
            TOOLS["cov"],
            "export",
            "-format=lcov",
            f"-instr-profile={profdata}",
            binary,
            f"-ignore-filename-regex={IGNORE}",
        ],
        check=True,
        capture_output=True,
        text=True,
    ).stdout
    profdata.unlink(missing_ok=True)
    rows = []
    prefix = str(REPO) + "/"
    current = ""
    for line in lcov.splitlines():
        if line.startswith("SF:"):
            current = line[3:].removeprefix(prefix)
        elif line.startswith("DA:"):
            number, count = line[3:].split(",")[:2]
            rows.append((current, int(number), int(count)))
    return rows


def build_line_table(binary: str, out: Path) -> list[tuple[str, int]]:
    """Every instrumented line, from a run that executes no test at all. The coverage mapping
    lives in the binary, so the export lists every line whether or not anything reached it."""
    work = out / "work"
    work.mkdir(parents=True, exist_ok=True)
    raw = work / "empty.profraw"
    subprocess.run(
        [binary, "--exact", "__no_test_has_this_name__"],
        env={**os.environ, "LLVM_PROFILE_FILE": str(raw)},
        cwd=REPO,
        capture_output=True,
        check=True,
    )
    rows = covered_lines(binary, raw)
    raw.unlink(missing_ok=True)
    table = sorted({(file, line) for file, line, _ in rows})
    with open(out / "lines.tsv", "w") as handle:
        handle.write("index\tarea\tfile\tline\n")
        for index, (file, line) in enumerate(table):
            handle.write(f"{index}\t{area_of(file)}\t{file}\t{line}\n")
    return table


def read_line_table(out: Path) -> list[tuple[str, int]]:
    with open(out / "lines.tsv") as handle:
        next(handle)
        return [(row[2], int(row[3])) for row in (line.rstrip("\n").split("\t") for line in handle)]


def measure(name: str, binary: str, out: Path, index: dict, args) -> dict:
    key = hashlib.sha1(name.encode()).hexdigest()[:16]
    raw = out / "work" / f"{key}.profraw"
    raw.unlink(missing_ok=True)
    command = [
        "systemd-run",
        "--user",
        "--scope",
        "--quiet",
        "--collect",
        f"--slice={SLICE}",
        "-p",
        f"MemoryMax={args.memory_max}",
        "-p",
        "MemorySwapMax=0",
        binary,
        "--exact",
        name,
        "--test-threads",
        "1",
    ]
    started = time.monotonic()
    try:
        result = subprocess.run(
            command,
            env={**os.environ, "LLVM_PROFILE_FILE": str(raw)},
            cwd=REPO,
            capture_output=True,
            text=True,
            timeout=args.timeout,
            check=False,  # a failing test is data, recorded in `status`, not an exception
        )
        code = result.returncode
        status = "ok" if code == 0 else ("failed" if code == 101 else "killed")
        # An `#[ignore]`d test selected with `--exact` is not run at all, and the binary still
        # exits 0 - so exit status alone would record it as a passing test that reached nothing,
        # and one such empty set makes the intersection over every test empty too. libtest's own
        # summary line says how many actually ran.
        summary = re.search(
            r"test result: \w+\. (\d+) passed; (\d+) failed; (\d+) ignored", result.stdout
        )
        if summary and summary.group(1) == "0" and summary.group(2) == "0":
            status = "ignored"
    except subprocess.TimeoutExpired:
        code, status = None, "timeout"
    seconds = round(time.monotonic() - started, 3)

    record = {"name": name, "status": status, "returncode": code, "seconds": seconds}
    if status == "ignored":
        raw.unlink(missing_ok=True)
        record["bits"] = None
        return record
    if not raw.exists():
        # A process killed before exit writes no profile, so there is nothing to record.
        record["bits"] = None
        return record

    bits = 0
    unknown = 0
    per_area: dict[str, int] = {}
    for file, line, count in covered_lines(binary, raw):
        if count == 0:
            continue
        position = index.get((file, line))
        if position is None:
            unknown += 1
            continue
        bits |= 1 << position
        area = area_of(file)
        per_area[area] = per_area.get(area, 0) + 1
    raw.unlink(missing_ok=True)

    bits_path = out / "bits" / key
    tmp = bits_path.with_suffix(".tmp")
    tmp.write_bytes(bits.to_bytes((len(index) + 7) // 8, "little"))
    tmp.replace(bits_path)
    record.update({"bits": key, "covered": per_area, "unknown_lines": unknown})
    return record


TOOLS: dict[str, str] = {}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--binary", required=True, help="the instrumented library test binary")
    parser.add_argument("--out", required=True, type=Path)
    parser.add_argument("--jobs", type=int, default=max(1, (os.cpu_count() or 2) - 1))
    parser.add_argument("--filter", default="", help="only tests whose name matches this regex")
    parser.add_argument("--timeout", type=int, default=1800, help="seconds per test")
    parser.add_argument("--memory-max", default="6G", help="cgroup memory cap per test process")
    args = parser.parse_args()

    TOOLS["profdata"] = llvm_tool("llvm-profdata")
    TOOLS["cov"] = llvm_tool("llvm-cov")
    binary = str(Path(args.binary).resolve())
    out = args.out.resolve()
    (out / "bits").mkdir(parents=True, exist_ok=True)
    (out / "work").mkdir(parents=True, exist_ok=True)

    table = read_line_table(out) if (out / "lines.tsv").exists() else build_line_table(binary, out)
    index = {entry: position for position, entry in enumerate(table)}
    print(f"{len(table)} instrumented lines outside the fixture stubs", flush=True)

    records_path = out / "records.jsonl"
    done = set()
    if records_path.exists():
        with open(records_path) as handle:
            done = {json.loads(line)["name"] for line in handle if line.strip()}

    pattern = re.compile(args.filter)
    todo = [
        name
        for name in list_tests(binary, out / "work")
        if pattern.search(name) and name not in done
    ]
    print(f"{len(done)} already measured, {len(todo)} to go, {args.jobs} at a time", flush=True)

    lock = threading.Lock()
    finished = 0
    started = time.monotonic()
    with (
        open(records_path, "a") as handle,
        concurrent.futures.ThreadPoolExecutor(args.jobs) as pool,
    ):
        futures = {pool.submit(measure, name, binary, out, index, args): name for name in todo}
        for future in concurrent.futures.as_completed(futures):
            try:
                record = future.result()
            except Exception as error:  # a harness failure, not a test failure - keep going
                record = {"name": futures[future], "status": "error", "error": repr(error)}
            with lock:
                handle.write(json.dumps(record, sort_keys=True) + "\n")
                handle.flush()
                finished += 1
                if record["status"] != "ok" or finished % 100 == 0:
                    rate = finished / max(time.monotonic() - started, 1e-9)
                    print(
                        f"{finished}/{len(todo)} ({rate:.1f}/s) last: {record['status']} "
                        f"{record['name']}",
                        flush=True,
                    )
    print("done", flush=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
