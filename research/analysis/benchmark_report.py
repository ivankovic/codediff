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
"""Summarise benchmark_diff_pairs results across all languages.

Reads results/benchmark_<language>.csv (or --results-dir), prints a percentile
table, and writes plots/benchmark_timing_distributions.png.

Usage (from research/):
    uv run ./analysis/benchmark_report.py
    uv run ./analysis/benchmark_report.py --results-dir results/ --plots-dir plots/
"""
import argparse
import csv
import re
from pathlib import Path

import numpy as np
import matplotlib.pyplot as plt
import matplotlib.ticker as ticker


# Language display order, display names, and hex colours.
LANG_META = {
    "rust":   ("Rust",   "#CE422B"),
    "python": ("Python", "#3572A5"),
    "go":     ("Go",     "#00ADD8"),
}


def read_rows(csv_path: Path) -> list[dict]:
    with open(csv_path, newline="") as f:
        return list(csv.DictReader(f))


def lang_key_from_path(path: Path) -> str | None:
    """Extract lower-case language key from benchmark_<language>.csv filename."""
    m = re.match(r"benchmark_([a-z]+)\.csv$", path.name)
    return m.group(1) if m else None


def compute_stats(rows: list[dict]) -> dict:
    ok = [r for r in rows if r["status"] == "ok"]
    skipped = sum(1 for r in rows if r["status"] == "skipped_too_large")
    timeout = sum(1 for r in rows if r["status"] == "timed_out")
    panicked = sum(1 for r in rows if r["status"] == "panicked")

    elapsed = np.array([float(r["elapsed_ms"]) for r in ok], dtype=float)

    stats = {
        "n_total": len(rows),
        "n_ok": len(ok),
        "n_skipped": skipped,
        "n_timeout": timeout,
        "n_panicked": panicked,
        "elapsed": elapsed,
    }
    if len(elapsed):
        p50, p75, p90, p99 = np.percentile(elapsed, [50, 75, 90, 99])
        stats.update({
            "mean":  elapsed.mean(),
            "p50":   p50,
            "p75":   p75,
            "p90":   p90,
            "p99":   p99,
            "max":   elapsed.max(),
        })
    else:
        stats.update({"mean": float("nan"), "p50": float("nan"), "p75": float("nan"),
                      "p90": float("nan"), "p99": float("nan"), "max": float("nan")})
    return stats


def print_table(results: list[tuple[str, str, dict]]) -> None:
    """Print a fixed-width percentile table to stdout."""
    col_w = {"lang": 8, "ok": 6, "skip": 7, "tout": 7, "panic": 6,
              "mean": 9, "p50": 8, "p75": 8, "p90": 9, "p99": 10, "max": 10}

    def row(lang, ok, skip, tout, panic, mean, p50, p75, p90, p99, mx):
        return (
            f"{lang:<{col_w['lang']}}  {ok:>{col_w['ok']}}  {skip:>{col_w['skip']}}"
            f"  {tout:>{col_w['tout']}}  {panic:>{col_w['panic']}}"
            f"  {mean:>{col_w['mean']}}  {p50:>{col_w['p50']}}  {p75:>{col_w['p75']}}"
            f"  {p90:>{col_w['p90']}}  {p99:>{col_w['p99']}}  {mx:>{col_w['max']}}"
        )

    header = row("Language", "ok", "skipped", "timeout", "panic",
                 "mean(ms)", "p50(ms)", "p75(ms)", "p90(ms)", "p99(ms)", "max(ms)")
    sep = "-" * len(header)
    print(sep)
    print(header)
    print(sep)
    for _key, display_name, s in results:
        def fmt(v):
            return f"{v:,.0f}" if not np.isnan(v) else "—"
        print(row(
            display_name,
            f"{s['n_ok']:,}", f"{s['n_skipped']:,}", f"{s['n_timeout']:,}", f"{s['n_panicked']:,}",
            fmt(s["mean"]), fmt(s["p50"]), fmt(s["p75"]), fmt(s["p90"]),
            fmt(s["p99"]), fmt(s["max"]),
        ))
    print(sep)


def plot_ecdf(results: list[tuple[str, str, dict]], output_path: Path) -> None:
    """Log-scale ECDF of elapsed_ms per language, with p50/p90 guide lines."""
    fig, ax = plt.subplots(figsize=(10, 6))
    ax.set_xscale("log")

    for lang_key, display_name, s in results:
        elapsed = s["elapsed"]
        if not len(elapsed):
            continue
        color = LANG_META.get(lang_key, (display_name, "#888888"))[1]
        sorted_ms = np.sort(elapsed)
        y = np.arange(1, len(sorted_ms) + 1) / len(sorted_ms)
        ax.step(sorted_ms, y, where="post", label=display_name,
                color=color, linewidth=2)

    # Horizontal guide lines at p50 and p90, labelled via axes-fraction x-coordinate
    for pct, ls in ((50, "--"), (90, ":")):
        ax.axhline(pct / 100, color="black", linewidth=0.7, linestyle=ls, alpha=0.35)
        ax.text(0.01, pct / 100 + 0.015, f"p{pct}",
                transform=ax.transAxes, fontsize=8, color="black", alpha=0.55,
                va="bottom")

    ax.xaxis.set_major_formatter(ticker.FuncFormatter(
        lambda x, _: f"{x:,.0f}" if x >= 1 else f"{x:.1f}"
    ))
    ax.set_xlabel("diff_code elapsed time (ms, log scale)", fontsize=12)
    ax.set_ylabel("Fraction of pairs ≤ x", fontsize=12)
    ax.set_title("diff_code timing distribution by language (ok pairs, 16k node limit)",
                 fontsize=13)
    ax.legend(fontsize=11)
    ax.grid(True, which="both", alpha=0.2)
    ax.set_ylim(0, 1.05)

    output_path.parent.mkdir(parents=True, exist_ok=True)
    fig.savefig(output_path, dpi=150, bbox_inches="tight")
    plt.close(fig)
    print(f"Plot saved to {output_path}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(
        description="Summarise benchmark_diff_pairs results across languages."
    )
    parser.add_argument(
        "--results-dir", default="results",
        help="Directory containing benchmark_<language>.csv files (default: results/)",
    )
    parser.add_argument(
        "--plots-dir", default="plots",
        help="Directory for output PNG (default: plots/)",
    )
    args = parser.parse_args()

    results_dir = Path(args.results_dir)
    plots_dir = Path(args.plots_dir)

    # Discover CSVs, sorted by canonical language order then alphabetically.
    order = list(LANG_META.keys())
    csv_files = sorted(
        results_dir.glob("benchmark_*.csv"),
        key=lambda p: (order.index(lang_key_from_path(p))
                       if lang_key_from_path(p) in order else 99, p.name),
    )

    if not csv_files:
        print(f"No benchmark_*.csv found in {results_dir}/")
        print("Run:  ./measure/benchmark_all.sh")
        raise SystemExit(1)

    results = []
    for csv_path in csv_files:
        lang_key = lang_key_from_path(csv_path)
        if lang_key is None:
            continue
        display_name, _ = LANG_META.get(lang_key, (lang_key.capitalize(), "#888888"))
        rows = read_rows(csv_path)
        s = compute_stats(rows)
        results.append((lang_key, display_name, s))
        print(f"Loaded {csv_path.name}: {len(rows)} rows")

    print()
    print_table(results)
    print()

    plot_path = plots_dir / "benchmark_timing_distributions.png"
    plot_ecdf(results, plot_path)
