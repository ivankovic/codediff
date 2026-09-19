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

"""Every modified code file in the corpus's recent history, as pairs `benchmark_diff_pairs` reads.

`edit_shape_stats.py` walks the same history and keeps only aggregates, on purpose (its class doc
comment says why a per-edit CSV of the whole corpus was ruled out for git). This is the per-edit
list that walk does not write, for the one consumer that needs it: the paper's robustness run
over the whole Full corpus rather than over the fixture corpus (`make measure-robustness-full`, which starts `measure/overnight_benchmarks.sh`).
The output is the shape `sample_code_pairs` produces (`language, size_bucket, repository, commit,
path, old_path`), so `benchmark_diff_pairs --csv` takes it unchanged; `size_bucket` is empty,
since nothing here is stratified.

Only modifications and renames are listed (`--diff-filter=MR`): a creation or deletion has no
pair of versions to diff, and `benchmark_diff_pairs` would only record it as unreadable. Shallow-
boundary commits are skipped for the reason `edit_shape_stats.numstat_rows` gives, and so is the
same 50-commit cap per repository, so this lists the population `edit_shape.csv` summarises.

Not a committed artifact: hundreds of thousands of rows that are entirely reproducible from the
clones. Write it under /var/tmp/research/<mode>/ and point the benchmark at it.
"""

import argparse
import csv
import os
import subprocess
import sys

from edit_shape_stats import language_of, shallow_boundary_commits

FIELDS = ["language", "size_bucket", "repository", "commit", "path", "old_path"]


def rename_sides(path):
    """`(old_path, new_path)` for a numstat path, which is the same path twice unless git wrote
    it as a rename: `before => after` whole, or `dir/{before => after}/file` per component."""
    if " => " not in path:
        return path, path
    old, new = path, path
    while "{" in old and "}" in old:
        head, _, rest = old.partition("{")
        group, _, tail = rest.partition("}")
        left, _, right = group.partition(" => ")
        old = head + left + tail
        new_head, _, new_rest = new.partition("{")
        new_group, _, new_tail = new_rest.partition("}")
        new = new_head + new_group.partition(" => ")[2] + new_tail
    if " => " in old:
        # No braces: the whole path was rewritten.
        old, new = old.split(" => ", 1)[0], new.split(" => ", 1)[1]
    # A component that is empty on one side (`a/{ => b}/c`, a move into or out of a directory)
    # leaves a doubled slash behind; git's own path has none. Missed until 2026-09-19, when
    # 5,789 of the first 47,719 R48 pairs failed to read for exactly this.
    return _collapse(old.strip()), _collapse(new.strip())


def _collapse(path):
    return "/".join(part for part in path.split("/") if part)


def modified_code_files(repo, max_commits):
    """`(commit, old_path, path)` for every modified or renamed code file in `repo`'s log."""
    shallow = shallow_boundary_commits(repo)
    proc = subprocess.Popen(
        ["git", "-C", repo, "log", "--no-merges", "--numstat", "--diff-filter=MR", "--format=C%H"]
        + ([f"-n{max_commits}"] if max_commits else []),
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
        text=True,
        errors="replace",
        bufsize=1 << 20,
    )
    commit = None
    try:
        for line in proc.stdout:
            line = line.rstrip("\n")
            if line.startswith("C"):
                commit = line[1:]
                continue
            if not line.strip() or commit is None or commit in shallow:
                continue
            parts = line.split("\t", 2)
            if len(parts) != 3 or parts[0] == "-" or parts[1] == "-":
                continue
            old_path, path = rename_sides(parts[2])
            if language_of(path):
                yield commit, old_path, path
    finally:
        proc.stdout.close()
        if proc.wait() != 0:
            print(f"note: git log failed in {os.path.basename(repo)}", file=sys.stderr)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repositories", default="/var/tmp/research/full/repositories")
    parser.add_argument("--max-commits", type=int, default=50)
    parser.add_argument("--output", required=True)
    args = parser.parse_args()

    if not os.path.isdir(args.repositories):
        sys.exit(f"no repositories under {args.repositories}")

    names = sorted(os.listdir(args.repositories))
    written = 0
    with open(args.output, "w", newline="") as f:
        writer = csv.writer(f)
        writer.writerow(FIELDS)
        for i, name in enumerate(names, 1):
            repo = os.path.join(args.repositories, name)
            if not os.path.isdir(repo):
                continue
            for commit, old_path, path in modified_code_files(repo, args.max_commits):
                writer.writerow([language_of(path), "", name, commit, path, old_path])
                written += 1
            if i % 100 == 0:
                print(f"  {i}/{len(names)} repositories, {written} pairs", file=sys.stderr)
    print(f"{written} modified code-file pairs written to {args.output}")


if __name__ == "__main__":
    main()
