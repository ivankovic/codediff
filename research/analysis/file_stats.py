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
import numpy as np
import matplotlib.pyplot as plt


def compute_percentiles_and_plot(df, column_name, output_filename):
    """
    Compute 50, 90, 99, 99.9, 99.99 percentiles and max for a column and create a distribution plot.

    Args:
        df: Polars DataFrame containing the data
        column_name: Name of the column to analyze
        output_filename: Base filename for the output plot (will be saved in plots/ directory)

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
    trim_threshold = percentiles["p90"]
    df_trimmed = df.filter(pl.col(column_name) <= trim_threshold)

    plt.hist(df_trimmed[column_name].to_numpy(), bins=50, edgecolor="black")

    title = f"Distribution of {column_name} (90th percentile filtered)"
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


def export_percentiles_to_csv(df, columns, output_filename):
    """
    Compute percentiles for multiple columns and export to CSV.

    Args:
        df: Polars DataFrame containing the data
        columns: List of column names to analyze
        output_filename: Path for the output CSV file
    """
    results = []

    # Compute percentiles for all languages combined
    for column in columns:
        percentiles = {}
        percentiles["metric"] = column
        percentiles["language"] = "All languages"
        percentiles["p50"] = df.select(pl.col(column).quantile(0.50)).item()
        percentiles["p90"] = df.select(pl.col(column).quantile(0.90)).item()
        percentiles["p99"] = df.select(pl.col(column).quantile(0.99)).item()
        percentiles["p999"] = df.select(pl.col(column).quantile(0.999)).item()
        percentiles["p9999"] = df.select(pl.col(column).quantile(0.9999)).item()
        percentiles["max"] = df.select(pl.col(column).max()).item()
        results.append(percentiles)

    # Compute percentiles per language
    languages = df.select(pl.col("language")).unique()["language"].to_list()
    for language in languages:
        language_df = df.filter(pl.col("language") == language)
        if len(language_df) > 0:
            for column in columns:
                percentiles = {}
                percentiles["metric"] = column
                percentiles["language"] = language
                percentiles["p50"] = language_df.select(pl.col(column).quantile(0.50)).item()
                percentiles["p90"] = language_df.select(pl.col(column).quantile(0.90)).item()
                percentiles["p99"] = language_df.select(pl.col(column).quantile(0.99)).item()
                percentiles["p999"] = language_df.select(pl.col(column).quantile(0.999)).item()
                percentiles["p9999"] = language_df.select(pl.col(column).quantile(0.9999)).item()
                percentiles["max"] = language_df.select(pl.col(column).max()).item()
                results.append(percentiles)

    # Create DataFrame and export to CSV
    result_df = pl.DataFrame(results)
    result_df.write_csv(output_filename)
    print(f"Percentiles exported to {output_filename}")


def load_data(db_path):
    """
    Load data from the database and perform initial transformations.

    Args:
        db_path: Path to the SQLite database

    Returns:
        DataFrame with loaded and transformed data
    """
    # Load CSV
    df = pl.read_database_uri(
        "SELECT * FROM files",
        f"sqlite://{db_path}",
    )

    # Add filename column
    df = df.with_columns(pl.col("path").str.split("/").list.last().alias("filename"))
    # Add extension column
    df = df.with_columns(
        pl.col("filename").str.split(".").list.last().alias("extension")
    )

    # Add category column from tip
    df = df.with_columns(
        pl.col("tip").str.split("(").list.first().fill_null("Unknown").alias("category")
    )

    # Extract project name from path
    df = df.with_columns(
        pl.col("path")
        .str.replace_all(r"/+", "/")
        .str.replace_all(r"/\./", "/")
        .str.strip_prefix("/var/tmp/research/small/repositories/")
        .str.strip_prefix("/var/tmp/research/full/repositories/")
        .str.split("/")
        .list.get(0)
        .alias("project")
    )

    return df


def load_node_kind_counts(db_path):
    """
    Load per-file AST node-kind counts (from `file_stats`' `node_kind_counts` table, one row per
    (file, TreeSitter node kind) pair) joined with each file's language.

    Args:
        db_path: Path to the SQLite database

    Returns:
        DataFrame with columns: language, kind, count (one row per file/kind pair; the same
        `kind` appears once per file it occurs in, not pre-aggregated across files).
    """
    return pl.read_database_uri(
        """
        SELECT f.language AS language, k.kind AS kind, k.count AS count
        FROM node_kind_counts k
        JOIN files f ON f.id = k.file_id
        WHERE f.language IS NOT NULL
        """,
        f"sqlite://{db_path}",
    )


def _format_kind_as_code(kind):
    """
    Render a TreeSitter node-kind string as a Markdown inline code span, safely handling kinds
    that are themselves made of backticks or pipes - e.g. Rust's `|` closure-argument delimiter,
    `||`/`||=` operators, or a grammar's literal backtick token. A naive f"`{kind}`" wrap breaks
    on such kinds: it prematurely closes the code span and, since `|` is also the Markdown table
    cell delimiter, corrupts the table structure itself.
    """
    max_backtick_run = 0
    current_run = 0
    for ch in kind:
        if ch == "`":
            current_run += 1
            max_backtick_run = max(max_backtick_run, current_run)
        else:
            current_run = 0

    fence = "`" * (max_backtick_run + 1)
    padding = " " if kind.startswith("`") or kind.endswith("`") else ""
    code_span = f"{fence}{padding}{kind}{padding}{fence}"

    # `|` is the table cell delimiter; it must be escaped even inside a code span, since table
    # parsing splits on unescaped pipes before inline formatting is applied.
    return code_span.replace("|", "\\|")


def write_top_node_kinds_by_language(
    node_kind_df, top_n=100, output_filename="top_node_kinds_by_language.md"
):
    """
    Compute the sorted distribution of AST node kinds per language (by total occurrence count
    summed across every file of that language) and write the top `top_n` kinds per language to a
    markdown file, one table per language.

    Args:
        node_kind_df: DataFrame as returned by load_node_kind_counts (columns: language, kind,
            count)
        top_n: How many of the most common node kinds to keep per language
        output_filename: Path to write the markdown report to

    Returns:
        DataFrame with the full (not just top_n) per-language node-kind distribution, columns:
        language, kind, total_count, pct_of_language - sorted by language then total_count
        descending. Useful for follow-up analysis beyond what the markdown table shows.
    """
    per_language_totals = node_kind_df.group_by("language").agg(
        pl.col("count").sum().alias("language_total")
    )

    distribution = (
        node_kind_df.group_by(["language", "kind"])
        .agg(pl.col("count").sum().alias("total_count"))
        .join(per_language_totals, on="language")
        .with_columns(
            (pl.col("total_count") / pl.col("language_total") * 100).alias("pct_of_language")
        )
        .sort(["language", "total_count"], descending=[False, True])
        .drop("language_total")
    )

    languages = sorted(distribution.select(pl.col("language")).unique()["language"].to_list())

    lines = [
        "# Most common AST node kinds per language",
        "",
        f"Top {top_n} TreeSitter node kinds by total occurrence count, per language, as a "
        "percentage of all counted nodes in that language.",
        "",
    ]

    for language in languages:
        language_df = distribution.filter(pl.col("language") == language).head(top_n)

        lines.append(f"## {language}")
        lines.append("")
        lines.append("| Rank | Node kind | Count | % of language |")
        lines.append("|---:|---|---:|---:|")
        for rank, row in enumerate(language_df.iter_rows(named=True), start=1):
            lines.append(
                f"| {rank} | {_format_kind_as_code(row['kind'])} | {row['total_count']:,} | "
                f"{row['pct_of_language']:.2f}% |"
            )
        lines.append("")

    with open(output_filename, "w") as f:
        f.write("\n".join(lines) + "\n")

    print(f"Top {top_n} node kinds per language written to {output_filename}")

    return distribution


def compute_full_dataset_stats(df):
    """
    Compute and display statistics for the full dataset.

    Args:
        df: DataFrame containing the full dataset
    """
    # Compute extra columns
    tip_counts = (
        df.group_by("category")
        .agg(pl.len().alias("count"))
        .sort("count", descending=True)
    )
    print("Type counts:")
    print(tip_counts)

    plt.figure(figsize=(10, 5))
    plt.pie(
        tip_counts["count"],
        labels=tip_counts["category"],
        autopct="%1.1f%%",
    )
    plt.title("File Types")
    plt.savefig("plots/tips.png", bbox_inches="tight")
    plt.close()

    undefined_tip = df.filter(pl.col("category") == "Unknown")
    undefined_tip_extensions = (
        undefined_tip.group_by("extension")
        .agg(pl.len().alias("count"))
        .sort("count", descending=True)
        .head(10)
    )
    print("Undefined file type top 10 extensions:")
    print(undefined_tip_extensions)

    project_count = df.select(pl.col("project").n_unique()).item()
    print(f"Projects: {project_count}")

    file_count = df.select(pl.len())
    print(f"Files: {file_count['len'][0]}")

    files_per_project = df.group_by("project").len().rename({"len": "file_count"})

    # Compute percentiles and create distribution plot for files per project
    files_per_project_percentiles = compute_percentiles_and_plot(
        files_per_project, "file_count", "files_per_project.png"
    )

    # Filter for the correlation analysis (keep original filtering logic)
    files_per_project = files_per_project.filter(
        pl.col("file_count") < files_per_project_percentiles["p90"]
    )


def filter_to_only_code(df):
    """
    Filter the dataset to only include code files.

    Args:
        df: DataFrame containing the full dataset

    Returns:
        DataFrame filtered to only code files
    """
    return df.filter(pl.col("category") == "Code")


def compute_code_only_stats(df):
    """
    Compute and display statistics for code-only files.

    Args:
        df: DataFrame containing only code files
    """
    language_agg = df.group_by("language").len().rename({"len": "count"})
    pdf = language_agg.to_pandas()

    print(f"Languages: {language_agg.count()[0]}")
    pdf_sorted = pdf.sort_values("count", ascending=False)

    # Top 9
    top9 = pdf_sorted.iloc[:9]
    print(top9)

    # Everything else summed
    other_count = pdf_sorted.iloc[9:]["count"].sum()

    plot_df = top9.copy()
    plot_df.loc[len(plot_df)] = ["Other", other_count]

    plt.figure(figsize=(8, 8))
    plt.pie(plot_df["count"], labels=plot_df["language"], autopct="%1.1f%%")
    plt.title("Files by Language")
    plt.tight_layout()

    language_output_path = "plots/language_distribution.png"
    os.makedirs(os.path.dirname(language_output_path), exist_ok=True)
    plt.savefig(language_output_path, dpi=300)
    print(f"Plot saved to {language_output_path}")

    bytes_percentiles = compute_percentiles_and_plot(
        df, "bytes", "bytes_distribution.png"
    )
    compute_percentiles_and_plot(df, "lines_of_code", "lines_of_code_distribution.png")
    ast_percentiles = compute_percentiles_and_plot(
        df, "ast_nodes", "ast_nodes_distribution.png"
    )

    non_empty_code = df.filter((pl.col("bytes") > 0) & (pl.col("ast_nodes") > 0))

    print(
        "Pearson correlation between bytes and ast_nodes: ",
        non_empty_code.select(pl.corr("ast_nodes", "bytes")).item(),
    )

    sample = non_empty_code.filter(
        (pl.col("bytes") <= bytes_percentiles["p99"])
        & (pl.col("ast_nodes") <= ast_percentiles["p99"])
    )
    sample = sample.sample(fraction=0.02, seed=4859)

    # Use the sample data for polyfit to avoid empty arrays
    if len(sample) > 0:
        m, b = np.polyfit(sample["bytes"], sample["ast_nodes"], 1)
        print(f"<ast nodes> = {m} * <bytes> + {b}")
        fit = m * sample["bytes"] + b
    else:
        print("Not enough data points for polynomial fit")
        m, b = 0, 0
        fit = sample["bytes"] * 0  # Create array of zeros

    plt.figure()
    # plt.hexbin(df["bytes"], df["ast_nodes"], gridsize=200)
    plt.scatter(sample["bytes"], sample["ast_nodes"], s=1)
    plt.plot(sample["bytes"], fit, color="red", linestyle="--")
    plt.title("Correlation of AST Nodes and Bytes (99th percentile removed)")
    plt.xlabel("Bytes")
    plt.ylabel("AST Nodes")
    plt.savefig("plots/ast_nodes_bytes_correlation.png")
    plt.close()

    # Export percentiles to CSV
    export_percentiles_to_csv(
        df, ["bytes", "ast_nodes", "lines_of_code"], "code_percentiles.csv"
    )


# Main execution
if __name__ == "__main__":
    # Get database path from command line argument or use default
    db_path = (
        sys.argv[1] if len(sys.argv) > 1 else "/var/tmp/research/tiny/stats.sqlite"
    )

    os.makedirs("plots", exist_ok=True)

    # Load and transform data
    df = load_data(db_path)

    # Compute full dataset statistics
    compute_full_dataset_stats(df)

    # Filter to only code files
    code_df = filter_to_only_code(df)

    # Compute code-only statistics
    compute_code_only_stats(code_df)

    # Distribution of AST node kinds per language, and the 100 most common per language
    node_kind_df = load_node_kind_counts(db_path)
    write_top_node_kinds_by_language(node_kind_df, top_n=100)
