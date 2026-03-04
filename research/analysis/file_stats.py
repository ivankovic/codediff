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
    Compute 50, 90, 99, 99.9, and 99.99 percentiles for a column and create a distribution plot.

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

    print(f"Percentiles for {column_name}:")
    print(f"  50th percentile:   {percentiles['p50']:,}")
    print(f"  90th percentile:   {percentiles['p90']:,}")
    print(f"  99th percentile:   {percentiles['p99']:,}")
    print(f"  99.9th percentile: {percentiles['p999']:,}")
    print(f"  99.99th percentile: {percentiles['p9999']:,}")

    # Create distribution plot
    plt.figure(figsize=(8, 8))

    # Trim to 99th percentile for better visualization
    trim_threshold = percentiles["p99"]
    df_trimmed = df.filter(pl.col(column_name) <= trim_threshold)

    plt.hist(df_trimmed[column_name].to_numpy(), bins=50, edgecolor="black")

    title = f"Distribution of {column_name} (99th percentile filtered)"
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


# Get database path from command line argument or use default
db_path = sys.argv[1] if len(sys.argv) > 1 else "/var/tmp/research/tiny/stats.sqlite"

os.makedirs("plots", exist_ok=True)

# Load CSV
df = pl.read_database_uri(
    "SELECT * FROM files",
    f"sqlite://{db_path}",
)

# Add filename column
df = df.with_columns(pl.col("path").str.split("/").list.last().alias("filename"))
# Add extension column
df = df.with_columns(pl.col("filename").str.split(".").list.last().alias("extension"))


# Add category and subcategory columns from tip
# Parse tip format like "Code(Build)" or "Data(Image)"
def parse_tip(tip):
    if tip is None or tip == "" or tip == "null":
        return ("Unknown", "")
    if "(" in tip and tip.endswith(")"):
        category = tip.split("(")[0]
        subcategory = tip.split("(")[1].rstrip(")")
        return (category, subcategory)
    else:
        # Fallback for old format
        return (tip, "")


df = df.with_columns(
    pl.col("tip").str.split("(").list.first().fill_null("Unknown").alias("category")
)

# Compute extra columns
tip_counts = (
    df.group_by("category").agg(pl.len().alias("count")).sort("count", descending=True)
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

# print("Undefined language 10 random files:")
# sample = undefined_tip.sample(30)
# for row in sample.iter_rows():
#    print(row)
# exit(1)

df = df.with_columns(
    pl.col("path")
    .str.replace_all(r"/+", "/")
    .str.replace_all(r"/\./", "/")
    .str.strip_prefix("/var/tmp/research/small/repositories/")
    .str.split("/")
    .list.get(0)
    .alias("project")
)

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

# Only Code
df = df.filter(pl.col("category") == "Code")

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

# Compute percentiles and create distribution plots for ast_nodes and bytes
ast_percentiles = compute_percentiles_and_plot(
    df, "ast_nodes", "ast_nodes_distribution.png"
)
bytes_percentiles = compute_percentiles_and_plot(
    df, "bytes", "bytes_distribution.png"
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