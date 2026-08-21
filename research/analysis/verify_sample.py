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
"""Checks that every (repository, commit) in a `sample_code_pairs` CSV still resolves against the
checkouts it is meant to be measured over, and reports the per-(language, bucket) cell counts.

Why this is a gate and not a report. A sample CSV stores *pointers* - `(language, size_bucket,
repository, commit, path)` - and the blobs are read back out of the checkouts at measurement time.
That keeps the corpus small, but it means a sample is only valid while the history it points into
still exists, and shallow clones that get re-fetched drop old commits continuously. On 2026-08-20
the committed sample had decayed to ~41% unreadable, and because the losses were concentrated in
whole repositories rather than spread evenly, the surviving pairs were not a random subset: two
large projects had lost every sampled commit. A measurement over that sample would have looked
perfectly healthy in its own output while silently answering a different question.

So: run this immediately after drawing a sample, before measuring anything against it. Exit status
is nonzero if any pair fails to resolve, which is what makes it usable as a pipeline stage.

Usage (from research/):
    uv run ./analysis/verify_sample.py --repo-root /var/tmp/research/full/repositories SAMPLE.csv
"""

import argparse
import collections
import csv
import os
import subprocess
import sys


def resolves(repo_path, commit):
    """Whether `commit` is present in the clone at `repo_path`. `cat-file -e <sha>^{commit}`
    rather than `rev-parse`: rev-parse happily echoes back a well-formed SHA that names no object,
    so it answers "does this look like a commit id" instead of "is this commit here"."""
    if not os.path.isdir(repo_path):
        return False
    result = subprocess.run(
        ["git", "-C", repo_path, "cat-file", "-e", f"{commit}^{{commit}}"],
        capture_output=True,
    )
    return result.returncode == 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("csv_path", help="sample CSV from sample_code_pairs")
    parser.add_argument("--repo-root", required=True, help="checkout root the sample is measured against")
    parser.add_argument(
        "--max-unresolved-pct", type=float, default=0.0,
        help="tolerated percentage of unresolvable pairs (default 0: a freshly drawn sample "
             "should resolve completely, and anything else means the corpus moved under it)",
    )
    args = parser.parse_args()

    with open(args.csv_path) as f:
        rows = list(csv.DictReader(f))
    if not rows:
        print(f"{args.csv_path} has no rows", file=sys.stderr)
        return 1

    # One check per (repository, commit), not per row: a commit typically appears in several rows
    # (one per changed path), and git process startup dominates this loop.
    commits = {(r["repository"], r["commit"]) for r in rows}
    cache = {
        key: resolves(os.path.join(args.repo_root, key[0]), key[1])
        for key in sorted(commits)
    }

    unresolved = [r for r in rows if not cache[(r["repository"], r["commit"])]]
    pct = 100.0 * len(unresolved) / len(rows)

    print(f"{args.csv_path}: {len(rows)} pairs, {len(commits)} distinct commits, "
          f"{len(set(r['repository'] for r in rows))} repositories")
    print(f"unresolved: {len(unresolved)} pairs ({pct:.2f}%)")

    # Per-repository, because that is the shape the failure took last time and an aggregate
    # percentage hides it: a corpus that loses 40% of its pairs evenly is a smaller sample, while
    # one that loses two whole repositories is a biased one.
    if unresolved:
        by_repo = collections.Counter(r["repository"] for r in unresolved)
        totals = collections.Counter(r["repository"] for r in rows)
        print("\nworst-affected repositories:")
        for repo, missing in by_repo.most_common(15):
            print(f"  {repo:<50} {missing:>5}/{totals[repo]:<5} unresolved")

    print("\nper-(language, bucket) cell counts:")
    cells = collections.Counter((r["language"], r["size_bucket"]) for r in rows)
    languages = sorted({r["language"] for r in rows})
    buckets = sorted({r["size_bucket"] for r in rows}, key=lambda b: int(b.split("-")[0].rstrip("+")))
    header = f"  {'language':<14}" + "".join(f"{b:>11}" for b in buckets)
    print(header)
    for language in languages:
        counts = "".join(f"{cells[(language, b)]:>11}" for b in buckets)
        print(f"  {language:<14}{counts}")

    empty = [(lang, b) for lang in languages for b in buckets if cells[(lang, b)] == 0]
    if empty:
        print(f"\n{len(empty)} empty cells (no such pairs exist in the corpus, not an error): "
              f"{', '.join(f'{l}/{b}' for l, b in empty[:12])}"
              f"{' ...' if len(empty) > 12 else ''}")

    if pct > args.max_unresolved_pct:
        print(f"\nFAIL: {pct:.2f}% unresolved exceeds the {args.max_unresolved_pct:.2f}% threshold.",
              file=sys.stderr)
        print("Do not measure against this sample - re-fetch the corpus and re-draw it.",
              file=sys.stderr)
        return 1

    print("\nOK: every sampled pair resolves against the checkouts.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
