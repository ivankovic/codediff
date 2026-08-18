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
import argparse
import csv
import os
import subprocess
import tempfile
from pathlib import Path

import numpy as np
from scipy import stats
import matplotlib.pyplot as plt


def git_show(repo_path, ref, path):
    result = subprocess.run(
        ["git", "-C", str(repo_path), "show", f"{ref}:{path}"],
        capture_output=True,
    )
    return result.stdout if result.returncode == 0 else None


def unix_diff_line_counts(before, after):
    # Counts lines as the unix `diff` tool itself reports them: "< " is a removed line, "> "
    # is an added line, in its default (non-unified) output format.
    with (
        tempfile.NamedTemporaryFile() as before_file,
        tempfile.NamedTemporaryFile() as after_file,
    ):
        before_file.write(before)
        before_file.flush()
        after_file.write(after)
        after_file.flush()

        result = subprocess.run(
            ["diff", before_file.name, after_file.name],
            capture_output=True,
            text=True,
            errors="replace",
        )

        added = sum(1 for line in result.stdout.splitlines() if line.startswith("> "))
        removed = sum(1 for line in result.stdout.splitlines() if line.startswith("< "))
        return added, removed


def collect_pair_stats(csv_path, repo_root):
    rows = []
    skipped = 0

    with open(csv_path, newline="") as f:
        for row in csv.DictReader(f):
            repo_path = repo_root / row["repository"]
            before = git_show(repo_path, f"{row['commit']}^", row["old_path"])
            after = git_show(repo_path, row["commit"], row["path"])

            if before is None or after is None:
                skipped += 1
                continue

            added, removed = unix_diff_line_counts(before, after)
            rows.append(
                {
                    "language": row["language"],
                    "size_bucket": row["size_bucket"],
                    "repository": row["repository"],
                    "commit": row["commit"],
                    "path": row["path"],
                    "bytes_before": len(before),
                    "bytes_after": len(after),
                    "file_size": max(len(before), len(after)),
                    "lines_added": added,
                    "lines_removed": removed,
                    "loc_changed": added + removed,
                }
            )

    if skipped:
        print(f"Skipped {skipped} pairs that could not be read from the repository")

    return rows


def write_pair_stats_csv(rows, output_path):
    with open(output_path, "w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=list(rows[0].keys()))
        writer.writeheader()
        writer.writerows(rows)
    print(f"Per-pair stats written to {output_path}")


def describe(name, values):
    arr = np.asarray(values, dtype=float)
    described = stats.describe(arr)
    p50, p90, p99 = np.percentile(arr, [50, 90, 99])

    print(f"{name}:")
    print(f"  n={described.nobs}  mean={described.mean:.1f}  std={described.variance**0.5:.1f}")
    print(f"  skew={stats.skew(arr):.2f}  p50={p50:.0f}  p90={p90:.0f}  p99={p99:.0f}  max={arr.max():.0f}")

    return arr


def plot_log_distribution(arr, title, xlabel, output_path):
    # Both file sizes and LOC changed are strongly right-skewed (most diffs are small, a long
    # tail is large), so a log-spaced histogram with a fitted lognormal is the readable view.
    positive = arr[arr > 0]
    shape, loc, scale = stats.lognorm.fit(positive, floc=0)

    plt.figure(figsize=(8, 6))
    bins = np.logspace(np.log10(positive.min()), np.log10(positive.max()), 40)
    plt.hist(positive, bins=bins, density=True, edgecolor="black", alpha=0.7, label="observed")

    x = np.logspace(np.log10(positive.min()), np.log10(positive.max()), 200)
    plt.plot(x, stats.lognorm.pdf(x, shape, loc, scale), "r--", label=f"lognormal fit (σ={shape:.2f})")

    plt.xscale("log")
    plt.title(title)
    plt.xlabel(xlabel)
    plt.ylabel("Density")
    plt.legend()

    os.makedirs(os.path.dirname(output_path), exist_ok=True)
    plt.savefig(output_path, dpi=300, bbox_inches="tight")
    plt.close()
    print(f"Plot saved to {output_path}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(
        description="Compute and visualize size/LOC-changed statistics for sampled code pairs."
    )
    parser.add_argument("csv", nargs="?", default="data/samples/sampled_code_pairs_rust.csv")
    parser.add_argument("--repo-root", default="/var/tmp/research/small/repositories")
    parser.add_argument("--output-csv", default="data/corpus_stats/code_pair_diff_stats.csv")
    args = parser.parse_args()

    rows = collect_pair_stats(Path(args.csv), Path(args.repo_root))
    if not rows:
        raise SystemExit("No pairs could be read; nothing to analyze")

    write_pair_stats_csv(rows, args.output_csv)

    file_sizes = describe("File size (bytes)", [r["file_size"] for r in rows])
    loc_changed = describe("LOC changed (unix diff)", [r["loc_changed"] for r in rows])

    plot_log_distribution(
        file_sizes,
        "Distribution of file sizes",
        "File size (bytes, log scale)",
        "plots/code_pair_file_size_distribution.png",
    )
    plot_log_distribution(
        loc_changed,
        "Distribution of LOC changed (unix diff)",
        "Lines changed (log scale)",
        "plots/code_pair_loc_changed_distribution.png",
    )
