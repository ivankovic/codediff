# This file is part of the CodeDiff code diffing tool.
#
# Copyright (C) 2026 Marko Ivankovic
#
# This program is free software: you can redistribute it and/or modify
# it under the terms of the GNU Affero General Public License as published
# by the Free Software Foundation, either version 3 of the License, or
# (at your option) any later version.
#
# This program is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
# GNU Affero General Public License for more details.
#
# You should have received a copy of the GNU Affero General Public License
# along with this program. If not, see <https://www.gnu.org/licenses/>.

"""What every report script under research/analysis/ used to carry its own copy of.

Importable as a sibling module (`from _common import ...`) because `uv run ./analysis/foo.py`
puts the script's own directory first on `sys.path` - the same mechanism `file_stats.py` already
relies on for `percentile_report`. Kept to helpers with exactly one correct implementation: a CSV
reader, the papers' LaTeX number format, the repository paths, and the chart chrome every figure
shares. Anything a report computes differently from its siblings on purpose (the various `pct`
functions, say) stays in the report.
"""

import csv
from pathlib import Path

# research/analysis/_common.py -> research/ -> the repository root.
RESEARCH_DIR = Path(__file__).resolve().parents[1]
REPO_ROOT = RESEARCH_DIR.parent


def read_rows(csv_path: Path | str) -> list[dict]:
    """Every row of `csv_path` as a dict keyed by the header, all values as strings."""
    with open(csv_path, newline="") as f:
        return list(csv.DictReader(f))


def read_rows_with_fields(csv_path: Path | str) -> tuple[list[str], list[dict]]:
    """[`read_rows`] plus the header, in file order, for readers that derive their column set
    from whatever the producer wrote (e.g. one `<tool>_mismatches` column per external tool)."""
    with open(csv_path, newline="") as f:
        reader = csv.DictReader(f)
        rows = list(reader)
        return reader.fieldnames or [], rows


def latex_number(value: int) -> str:
    """An int with this project's papers' LaTeX-safe thousands separator: 1234567 ->
    "1{,}234{,}567". A plain comma can trigger LaTeX's comma-in-math spacing rules even in text
    mode (see research/papers/introductory-paper/main.tex)."""
    return f"{value:,}".replace(",", "{,}")


# Chart chrome, from the dataviz skill's reference palette (light mode) - identical across every
# figure so the paper's plots read as one system.
SURFACE = "#fcfcfb"
INK_PRIMARY = "#0b0b0b"
INK_SECONDARY = "#52514e"
INK_MUTED = "#898781"
GRIDLINE = "#e1e0d9"
