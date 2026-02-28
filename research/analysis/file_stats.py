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
import polars as pl
import numpy as np
import matplotlib.pyplot as plt

os.makedirs("plots", exist_ok=True)

# Load CSV
df = pl.read_database_uri(
    "SELECT * FROM files",
    "sqlite:///var/tmp/research/tiny/stats.sqlite",
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

plt.figure()
plt.pie(
    tip_counts["count"],
    labels=tip_counts["category"],
    autopct="%1.1f%%",
)
plt.legend(tip_counts["category"], loc="center left", bbox_to_anchor=(1, 0.5))
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
    .str.strip_prefix("/tmp/research/repositories/")
    .str.split("/")
    .list.get(0)
    .alias("project")
)

project_count = df.select(pl.col("project").n_unique()).item()
print(f"Projects: {project_count}")

file_count = df.select(pl.len())
print(f"Files: {file_count['len'][0]}")

files_per_project = df.group_by("project").len().rename({"len": "file_count"})
files_per_project_p90 = files_per_project.select(
    pl.col("file_count").quantile(0.90)
).item()

files_per_project = files_per_project.filter(
    pl.col("file_count") < files_per_project_p90
)

plt.hist(files_per_project["file_count"])
plt.xlabel("Number of files in project")
plt.ylabel("Number of projects")
plt.title("Distribution of file counts per project (Excluding 90+ percentile)")

language_output_path = "plots/files_per_project.png"
os.makedirs(os.path.dirname(language_output_path), exist_ok=True)
plt.savefig(language_output_path, dpi=300)
print(f"Plot saved to {language_output_path}")

# Only Code
df = df.filter(pl.col("category") == "Code")

language_agg = df.group_by("language").len().rename({"len": "count"})
pdf = language_agg.to_pandas()
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

# Compute 50th percentiles of sizes

p50_ast = df.select(pl.col("ast_nodes").quantile(0.50)).item()
p50_bytes = df.select(pl.col("bytes").quantile(0.50)).item()

print(f"50th percentile — ast_nodes: {p50_ast:,}")
print(f"50th percentile — bytes:     {p50_bytes:,}")

# Compute 99th percentiles
p99_ast = df.select(pl.col("ast_nodes").quantile(0.99)).item()
p99_bytes = df.select(pl.col("bytes").quantile(0.99)).item()
p999_bytes = df.select(pl.col("bytes").quantile(0.999)).item()
p9995_bytes = df.select(pl.col("bytes").quantile(0.99955555)).item()

print(f"99th percentile — ast_nodes: {p99_ast:,}")
print(f"99th percentile — bytes:     {p99_bytes:,}")
print(f"99.9th percentile — bytes:     {p999_bytes:,}")
print(f"99.95th percentile — bytes:     {p9995_bytes:,}")

# Trim values to ≤ 99th percentile (separately per metric)
df_ast_trim = df.filter(pl.col("ast_nodes") <= p99_ast)
df_bytes_trim = df.filter(pl.col("bytes") <= p99_bytes)

# Plot
plt.figure(figsize=(10, 5))

plt.subplot(1, 2, 1)
plt.hist(df_ast_trim["ast_nodes"].to_numpy(), bins=50, edgecolor="black")
plt.title("AST Nodes ≤ 99th percentile")
plt.xlabel("ast_nodes")
plt.ylabel("Frequency")
plt.xticks(rotation=30)

plt.subplot(1, 2, 2)
plt.hist(df_bytes_trim["bytes"].to_numpy(), bins=50, edgecolor="black")
plt.title("Bytes ≤ 99th percentile")
plt.xlabel("bytes")
plt.ylabel("Frequency")
plt.xticks(rotation=30)

plt.tight_layout()

size_output_path = "plots/ast_nodes_bytes_distribution.png"
os.makedirs(os.path.dirname(size_output_path), exist_ok=True)
plt.savefig(size_output_path, dpi=300)
print(f"Plot saved to {size_output_path}")

non_empty_code = df.filter((pl.col("bytes") > 0) & (pl.col("ast_nodes") > 0))

print(
    "Pearson correlation between bytes and ast_nodes: ",
    non_empty_code.select(pl.corr("ast_nodes", "bytes")).item(),
)

sample = non_empty_code.filter(
    (pl.col("bytes") <= p99_bytes) & (pl.col("ast_nodes") <= p99_ast)
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
plt.savefig("plots/ast_nodes_bytes_distribution.png")
plt.close()
