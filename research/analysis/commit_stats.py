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
import os
import sys

import matplotlib.pyplot as plt
import polars as pl
from percentile_report import compute_percentiles_and_plot


def load_data(db_path):
    """
    Load data from the database and perform initial transformations.

    Args:
        db_path: Path to the SQLite database

    Returns:
        DataFrame with loaded and transformed data
    """
    # Load from SQLite
    df = pl.read_database_uri(
        "SELECT * FROM commits",
        f"sqlite://{db_path}",
    )

    # Extract repository name from file path
    df = df.with_columns(
        pl.col("relative_file_path")
        .str.replace_all(r"/+", "/")
        .str.replace_all(r"/\./", "/")
        .str.split("/")
        .list.first()
        .alias("repository")
    )

    # Extract file extension
    df = df.with_columns(pl.col("relative_file_path").str.split(".").list.last().alias("extension"))

    # Derived metrics. Only the byte and line deltas: `commit_stats.rs` writes the six churn
    # columns (`lines_added`/`lines_removed`/`lines_changed`, `nodes_*`) as 0 for every row, so
    # anything aggregated from them is a distribution of zeros - `edit_shape_stats.py` is the
    # measurement of edit size (see data/README.md).
    df = df.with_columns(
        (pl.col("bytes_after") - pl.col("bytes_before")).alias("bytes_delta"),
        (pl.col("lines_after") - pl.col("lines_before")).alias("lines_delta"),
    )

    return df


def compute_basic_stats(df):
    """
    Compute and display basic statistics about the dataset.

    Args:
        df: DataFrame containing commit data
    """
    print("=== Basic Statistics ===")

    # Count unique commits and repositories
    unique_commits = df.select(pl.col("commit_id").n_unique()).item()
    unique_repos = df.select(pl.col("repository").n_unique()).item()
    total_rows = len(df)

    print(f"Unique commits: {unique_commits:,}")
    print(f"Unique repositories: {unique_repos:,}")
    print(f"Total (commit, file) pairs: {total_rows:,}")

    # Files per commit distribution
    files_per_commit = df.group_by("commit_id").agg(pl.len().alias("file_count"))
    files_per_commit_percentiles = compute_percentiles_and_plot(
        files_per_commit, "file_count", "files_per_commit.png", log_scale=True
    )

    # File change types distribution
    change_type_counts = (
        df.group_by("git_reported_status")
        .agg(pl.len().alias("count"))
        .sort("count", descending=True)
    )
    print("\nFile change types:")
    print(change_type_counts)

    plt.figure(figsize=(8, 8))
    plt.pie(
        change_type_counts["count"],
        labels=change_type_counts["git_reported_status"],
        autopct="%1.1f%%",
    )
    plt.title("File Change Types Distribution")
    plt.savefig("plots/change_types_distribution.png", dpi=300)
    plt.close()

    return files_per_commit_percentiles


def compute_change_magnitude_stats(df):
    """
    Compute statistics about the magnitude of changes.

    Args:
        df: DataFrame containing commit data
    """
    print("\n=== Change Magnitude Statistics ===")

    # Bytes changed
    bytes_percentiles = compute_percentiles_and_plot(
        df, "bytes_delta", "bytes_delta_distribution.png", log_scale=True
    )

    # Lines changed
    lines_percentiles = compute_percentiles_and_plot(
        df, "lines_delta", "lines_delta_distribution.png", log_scale=True
    )

    # Unix diff script size
    diff_size_percentiles = compute_percentiles_and_plot(
        df,
        "unix_diff_script_bytes",
        "unix_diff_script_size_distribution.png",
        log_scale=True,
    )

    return {
        "bytes": bytes_percentiles,
        "lines": lines_percentiles,
        "diff_size": diff_size_percentiles,
    }


def compute_language_stats(df):
    """
    Compute statistics grouped by programming language.

    Args:
        df: DataFrame containing commit data
    """
    print("\n=== Language Statistics ===")

    # Count commits by language
    commits_by_language = (
        df.group_by("language")
        .agg(pl.len().alias("commit_count"))
        .sort("commit_count", descending=True)
        .filter(pl.col("language") != "Unknown")
        .head(10)
    )

    print("Top 10 languages by commit count:")
    print(commits_by_language)

    # Plot top languages
    plt.figure(figsize=(10, 6))
    plt.bar(commits_by_language["language"], commits_by_language["commit_count"])
    plt.title("Top 10 Languages by Commit Count")
    plt.xlabel("Language")
    plt.ylabel("Number of Commits")
    plt.xticks(rotation=45)
    plt.tight_layout()
    plt.savefig("plots/top_languages_by_commits.png", dpi=300)
    plt.close()


def compute_repository_stats(df):
    """
    Compute statistics grouped by repository.

    Args:
        df: DataFrame containing commit data
    """
    print("\n=== Repository Statistics ===")

    # Commits per repository
    commits_per_repo = (
        df.group_by("repository")
        .agg(pl.len().alias("commit_count"))
        .sort("commit_count", descending=True)
        .head(10)
    )

    print("Top 10 repositories by commit count:")
    print(commits_per_repo)


if __name__ == "__main__":
    # Get database path from command line argument or use default
    db_path = sys.argv[1] if len(sys.argv) > 1 else "/var/tmp/research/small/stats.sqlite"

    os.makedirs("plots", exist_ok=True)

    print(f"Loading data from {db_path}...")

    # Load and transform data
    df = load_data(db_path)

    print(f"Loaded {len(df):,} (commit, file) pairs")

    # Compute basic statistics
    files_per_commit_percentiles = compute_basic_stats(df)

    # Compute change magnitude statistics
    change_stats = compute_change_magnitude_stats(df)

    # Compute language statistics
    compute_language_stats(df)

    # Compute repository statistics
    compute_repository_stats(df)

    print("\nAnalysis complete!")
