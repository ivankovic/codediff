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
#
# Shared by commit_stats.py and file_stats.py.
import os
import polars as pl
import matplotlib.pyplot as plt


def compute_percentiles_and_plot(
    df, column_name, output_filename, log_scale=False, trim_percentile="p99"
):
    """
    Compute 50, 90, 99, 99.9, 99.99 percentiles and max for a column and create a distribution plot.

    Args:
        df: Polars DataFrame containing the data
        column_name: Name of the column to analyze
        output_filename: Base filename for the output plot (will be saved in plots/ directory)
        log_scale: Whether to use log scale for the plot
        trim_percentile: Which computed percentile ("p50"/"p90"/"p99"/"p999"/"p9999") to trim the
            plotted distribution to, for readability

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

    # Trim for better visualization
    trim_threshold = percentiles[trim_percentile]
    df_trimmed = df.filter(pl.col(column_name) <= trim_threshold)

    if log_scale:
        plt.hist(
            df_trimmed[column_name].to_numpy(), bins=50, edgecolor="black", log=True
        )
    else:
        plt.hist(df_trimmed[column_name].to_numpy(), bins=50, edgecolor="black")

    trim_label = trim_percentile.lstrip("p")
    title = f"Distribution of {column_name} ({trim_label}th percentile filtered)"
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
