#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
#  This file is part of the OmniDiff code diffing tool.
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
What the files a diff tool is asked about actually are: every file changed by the most recent
`--max-commits` non-merge commits of each clone, counted by extension (or by name, for files
without one) and by the file-type category of `code::tip` - the classifier behind the introductory
paper's Figure 1, which counts files at rest. This counts files in motion, which is what a diff
tool sees.

Population: the same window as `edit_shape_stats.py`, through its `numstat_rows` (shallow-boundary
commits skipped, renames attributed to the new path), but binary files kept: git's own binary
verdict (`--numstat` reports `-` for a binary file) is the `binary_changes` column.

Count repositories, not only changes. The volume is dominated by a handful of repositories: UFO font
sources (`.glif`, XML) are hundreds of thousands of changes from ten repositories, so the
`repositories` column - how many clones changed such a file at all - is what says how common a
format is.

Categories come from `classify_paths` (the Rust classifier, built by the Makefile target), applied
to one representative path per key.

Usage (from research/):  uv run ./analysis/change_census.py [--repositories DIR] [--jobs N]
"""

import argparse
import collections
import csv
import os
import subprocess
import sys
from concurrent.futures import ProcessPoolExecutor

from edit_shape_stats import numstat_rows


def key_of(path):
    """The census key for `path`: its lowercased extension with the dot (`.png`), `.min.js` for
    minified JavaScript, or the lowercased file name when it has no extension (`makefile`)."""
    name = path.rsplit("/", 1)[-1].lower()
    if name.endswith(".min.js"):
        return ".min.js"
    stem = name.strip(".")
    if "." in stem:
        return "." + stem.rsplit(".", 1)[1]
    return name


def census_of(repo, max_commits):
    """One repository's tallies: changes, binary changes and lines changed per key, a
    representative path per key, and how many commits contributed."""
    changes = collections.Counter()
    binary = collections.Counter()
    lines = collections.Counter()
    sample = {}
    commits = set()
    for commit, path, added, removed in numstat_rows(repo, max_commits, keep_binary=True):
        key = key_of(path)
        commits.add(commit)
        changes[key] += 1
        sample.setdefault(key, path)
        if added is None:
            binary[key] += 1
        else:
            lines[key] += added + removed
    return changes, binary, lines, sample, len(commits)


def _census_of(args):
    return census_of(*args)


def classify(paths, classifier):
    """`code::tip`'s category for each path, via `classify_paths`; empty when it has none."""
    result = subprocess.run(
        [classifier], input="\n".join(paths) + "\n", capture_output=True, text=True, check=True
    )
    categories = {}
    for line in result.stdout.splitlines():
        path, _, category = line.partition("\t")
        categories[path] = category
    return categories


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repositories", default="/var/tmp/research/small/repositories")
    parser.add_argument("--max-commits", type=int, default=50)
    parser.add_argument("--jobs", type=int, default=os.cpu_count())
    parser.add_argument(
        "--classifier",
        default=os.path.join(
            os.path.dirname(__file__), "..", "..", "target", "release", "classify_paths"
        ),
    )
    parser.add_argument("--output", default=None)
    args = parser.parse_args()
    research_dir = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    output = args.output or os.path.join(research_dir, "data", "corpus_stats", "change_census.csv")

    repos = sorted(
        os.path.join(args.repositories, name)
        for name in os.listdir(args.repositories)
        if os.path.isdir(os.path.join(args.repositories, name, ".git"))
    )
    changes, binary, lines, repositories = (collections.Counter() for _ in range(4))
    sample = {}
    commits = 0
    with ProcessPoolExecutor(args.jobs) as pool:
        work = ((repo, args.max_commits) for repo in repos)
        for c, b, n_lines, s, n in pool.map(_census_of, work, chunksize=4):
            changes.update(c)
            binary.update(b)
            lines.update(n_lines)
            repositories.update(c.keys())
            for key, path in s.items():
                sample.setdefault(key, path)
            commits += n

    categories = classify([sample[key] for key in changes], args.classifier)
    with open(output, "w", newline="") as f:
        writer = csv.writer(f)
        writer.writerow(
            ["key", "category", "changes", "binary_changes", "lines_changed", "repositories"]
        )
        for key, count in sorted(changes.items(), key=lambda item: (-item[1], item[0])):
            writer.writerow(
                [
                    key,
                    categories.get(sample[key], ""),
                    count,
                    binary[key],
                    lines[key],
                    repositories[key],
                ]
            )

    total = sum(changes.values())
    print(
        f"{len(repos)} repositories, {commits} commits, {total} changed files "
        f"({100 * sum(binary.values()) / max(total, 1):.1f}% binary) -> {output}",
        file=sys.stderr,
    )


if __name__ == "__main__":
    main()
