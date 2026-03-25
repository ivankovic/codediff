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
import polars as pl
import matplotlib.pyplot as plt


def compute_percentiles_and_plot(df, column_name, output_filename, log_scale=False):
    """
    Compute 50, 90, 99, 99.9, 99.99 percentiles and max for a column and create a distribution plot.

    Args:
        df: Polars DataFrame containing the data
        column_name: Name of the column to analyze
        output_filename: Base filename for the output plot (will be saved in plots/ directory)
        log_scale: Whether to use log scale for the plot

    Returns:
        Dictionary containing the computed percentiles
    """
    # Compute percentiles
    percentiles = {}
    percentiles["p50"] = df.select(pl.col(column_name).quantile(0.50)).item()
    percentiles["p90"] = df.select(pl.col(column_name).quantile(0.90)).item()
    percentiles["p99"] = df.select(pl.col(column_name).quantile(0.99)).item()
    percentiles["p999"] = df.select(pl.col(column_name).quantile(0.999)).item()
    percentiles["p9999"] = df.select(pl.col(column_name).quantile(0.9999)).item()
    percentiles["max"] = df.select(pl.col(column_name).max()).item()

    print(f"Percentiles for {column_name}:")
    print(f"  50th percentile:   {percentiles['p50']:,}")
    print(f"  90th percentile:   {percentiles['p90']:,}")
    print(f"  99th percentile:   {percentiles['p99']:,}")
    print(f"  99.9th percentile: {percentiles['p999']:,}")
    print(f"  99.99th percentile: {percentiles['p9999']:,}")
    print(f"  max:               {percentiles['max']:,}")

    # Create distribution plot
    plt.figure(figsize=(8, 8))

    # Trim to 99th percentile for better visualization
    trim_threshold = percentiles["p99"]
    df_trimmed = df.filter(pl.col(column_name) <= trim_threshold)

    if log_scale:
        plt.hist(
            df_trimmed[column_name].to_numpy(), bins=50, edgecolor="black", log=True
        )
    else:
        plt.hist(df_trimmed[column_name].to_numpy(), bins=50, edgecolor="black")

    title = f"Distribution of {column_name} (99th percentile filtered)"
    if log_scale:
        title += " (Log Scale)"
        plt.yscale("log")
    plt.title(title)
    plt.xlabel(column_name)
    plt.ylabel("Frequency")
    plt.xticks(rotation=30)

    # Save plot
    output_path = f"plots/{output_filename}"
    os.makedirs(os.path.dirname(output_path), exist_ok=True)
    plt.savefig(output_path, dpi=300)
    plt.close()

    print(f"Plot saved to {output_path}")

    return percentiles


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
    df = df.with_columns(
        pl.col("relative_file_path").str.split(".").list.last().alias("extension")
    )

    # Compute derived metrics
    df = df.with_columns(
        (pl.col("bytes_after") - pl.col("bytes_before")).alias("bytes_delta"),
        (pl.col("lines_after") - pl.col("lines_before")).alias("lines_delta"),
        (pl.col("nodes_after") - pl.col("nodes_before")).alias("nodes_delta"),
        (
            pl.col("lines_added") + pl.col("lines_removed") + pl.col("lines_changed")
        ).alias("total_lines_modified"),
        (
            pl.col("nodes_added") + pl.col("nodes_removed") + pl.col("nodes_changed")
        ).alias("total_nodes_modified"),
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

    # Nodes changed
    nodes_percentiles = compute_percentiles_and_plot(
        df, "nodes_delta", "nodes_delta_distribution.png", log_scale=True
    )

    # Total lines modified
    total_lines_percentiles = compute_percentiles_and_plot(
        df,
        "total_lines_modified",
        "total_lines_modified_distribution.png",
        log_scale=True,
    )

    # Total nodes modified
    total_nodes_percentiles = compute_percentiles_and_plot(
        df,
        "total_nodes_modified",
        "total_nodes_modified_distribution.png",
        log_scale=True,
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
        "nodes": nodes_percentiles,
        "total_lines": total_lines_percentiles,
        "total_nodes": total_nodes_percentiles,
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

    # Language-specific change magnitude
    language_change_stats = (
        df.filter(pl.col("language") != "Unknown")
        .group_by("language")
        .agg(
            pl.col("total_lines_modified").mean().alias("avg_lines_modified"),
            pl.col("total_nodes_modified").mean().alias("avg_nodes_modified"),
            pl.len().alias("commit_count"),
        )
        .sort("commit_count", descending=True)
        .head(10)
    )

    print("\nAverage change magnitude by language (top 10):")
    print(language_change_stats)


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

    # Average change magnitude per repository
    repo_change_stats = (
        df.group_by("repository")
        .agg(
            pl.col("total_lines_modified").mean().alias("avg_lines_modified"),
            pl.col("total_nodes_modified").mean().alias("avg_nodes_modified"),
            pl.len().alias("commit_count"),
        )
        .sort("commit_count", descending=True)
        .head(10)
    )

    print("\nAverage change magnitude by repository (top 10):")
    print(repo_change_stats)


if __name__ == "__main__":
    # Get database path from command line argument or use default
    db_path = (
        sys.argv[1] if len(sys.argv) > 1 else "/var/tmp/research/small/stats.sqlite"
    )

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
