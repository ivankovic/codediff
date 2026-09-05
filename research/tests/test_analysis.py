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

"""Unit tests for the pure functions the report scripts are built from. Run with `make
test-python` from the repository root (or `uv run pytest` from research/)."""

import _common
import apted_only_report
import benchmark_other_report
import ci_local
import coverage_report
import edit_shape_stats
import numpy as np
import pytest

# --- _common -----------------------------------------------------------------------------------


def test_latex_number_uses_the_papers_thousands_separator():
    assert _common.latex_number(0) == "0"
    assert _common.latex_number(999) == "999"
    assert _common.latex_number(1000) == "1{,}000"
    assert _common.latex_number(1234567) == "1{,}234{,}567"


def test_read_rows_keeps_every_value_as_a_string(tmp_path):
    path = tmp_path / "rows.csv"
    path.write_text("name,count\nalpha,1\nbeta,\n")
    assert _common.read_rows(path) == [
        {"name": "alpha", "count": "1"},
        {"name": "beta", "count": ""},
    ]


def test_read_rows_with_fields_returns_the_header_in_file_order(tmp_path):
    path = tmp_path / "rows.csv"
    path.write_text("z_mismatches,a_mismatches\n1,2\n")
    fields, rows = _common.read_rows_with_fields(path)
    assert fields == ["z_mismatches", "a_mismatches"]
    assert rows == [{"z_mismatches": "1", "a_mismatches": "2"}]


def test_read_rows_with_fields_on_an_empty_file(tmp_path):
    path = tmp_path / "empty.csv"
    path.write_text("")
    assert _common.read_rows_with_fields(path) == ([], [])


def test_repo_root_is_the_directory_holding_research():
    assert (_common.REPO_ROOT / "research").is_dir()
    assert _common.RESEARCH_DIR == _common.REPO_ROOT / "research"


# --- apted_only_report -------------------------------------------------------------------------


def test_bucket_index_puts_each_loc_in_the_first_bucket_whose_bound_exceeds_it():
    # `stats::sampling::loc_bucket`'s rule: the first bucket whose *exclusive* upper bound the
    # value is strictly below, so a value equal to a bound belongs to the next bucket.
    loc = np.array([0, 9, 10, 29, 30, 299, 300, 2999, 3000, 10_000])
    assert list(apted_only_report.bucket_index(loc)) == [0, 0, 1, 1, 2, 3, 4, 5, 6, 6]


def test_bucket_label_formats_the_open_top_bucket_with_a_plus():
    assert apted_only_report.bucket_label(0, 10) == "0–10"
    assert apted_only_report.bucket_label(1000, 3000) == "1,000–3,000"
    assert apted_only_report.bucket_label(3000, float("inf")) == "3,000+"


# --- benchmark_other_report --------------------------------------------------------------------


def test_tool_names_come_from_the_mismatch_columns_in_order():
    fields = ["solution", "difft_mismatches", "difft_ms", "gumtree_mismatches", "codediff_ms"]
    assert benchmark_other_report.tool_names(fields) == ["difft", "gumtree"]


def test_ms_values_splits_the_semicolon_joined_repeats():
    assert benchmark_other_report.ms_values({"difft_ms": "12.5;13;12.75"}, "difft_ms") == [
        12.5,
        13.0,
        12.75,
    ]
    assert benchmark_other_report.ms_values({"difft_ms": ""}, "difft_ms") == []
    assert benchmark_other_report.ms_values({}, "difft_ms") == []


def test_applicable_rows_drops_fixtures_the_tool_did_not_score():
    rows = [{"t_mismatches": "0"}, {"t_mismatches": ""}, {"t_mismatches": "3"}]
    assert benchmark_other_report.applicable_rows(rows, "t") == [rows[0], rows[2]]


def test_common_subset_keeps_only_fixtures_every_tool_scored():
    rows = [
        {"a_status": "ok", "b_status": "line_only"},
        {"a_status": "ok", "b_status": "unsupported"},
        {"a_status": "ok"},
    ]
    assert benchmark_other_report.common_subset(rows, ["a", "b"]) == [rows[0]]
    assert benchmark_other_report.common_subset(rows, ["a"]) == rows


def test_pct_is_zero_where_the_total_is_zero():
    mismatches = np.array([1, 0, 5])
    total = np.array([4, 0, 0])
    assert list(benchmark_other_report.pct(mismatches, total)) == [25.0, 0.0, 0.0]


# --- edit_shape_stats --------------------------------------------------------------------------


@pytest.mark.parametrize(
    ("values", "q", "expected"),
    [
        ([], 50, None),
        ([7], 0, 7),
        ([7], 100, 7),
        ([1, 2, 3, 4, 5], 0, 1),
        ([1, 2, 3, 4, 5], 50, 3),
        ([1, 2, 3, 4, 5], 100, 5),
        ([1, 2, 3, 4], 90, 4),
    ],
)
def test_percentile_by_nearest_rank(values, q, expected):
    assert edit_shape_stats.percentile(values, q) == expected


# --- scripts/ci_local.py -----------------------------------------------------------------------


def test_expand_substitutes_matrix_and_env_expressions():
    text = 'cargo build --features "${{ matrix.features }}" ${{ env.FLAGS }}'
    assert (
        ci_local.expand(text, {"features": "stats"}, {"FLAGS": "--locked"})
        == 'cargo build --features "stats" --locked'
    )


def test_expand_treats_a_missing_env_value_as_empty_and_a_missing_matrix_value_as_an_error():
    assert ci_local.expand("x ${{ env.NOPE }} y", {}, {}) == "x  y"
    with pytest.raises(RuntimeError):
        ci_local.expand("${{ matrix.nope }}", {}, {})


def test_expand_refuses_expressions_it_cannot_evaluate():
    with pytest.raises(RuntimeError):
        ci_local.expand("${{ github.sha }}", {}, {})
    with pytest.raises(RuntimeError):
        ci_local.expand("${{ matrix.x", {"x": "1"}, {})


def test_matrix_combinations_is_the_cartesian_product_of_the_list_axes():
    job = {"strategy": {"matrix": {"features": ["", "stats"], "os": ["linux", "mac"]}}}
    assert ci_local.matrix_combinations(job) == [
        {"features": "", "os": "linux"},
        {"features": "", "os": "mac"},
        {"features": "stats", "os": "linux"},
        {"features": "stats", "os": "mac"},
    ]
    assert ci_local.matrix_combinations({}) == [{}]


# --- scripts/coverage_report.py ----------------------------------------------------------------


def test_area_of_picks_the_most_specific_prefix():
    assert coverage_report.area_of("src/diff/apted/engine.rs") != "other"
    assert coverage_report.area_of("src/bin/human_solver/main.rs") == "bin/ - dev tools"
    assert coverage_report.area_of("benches/diff_code_benchmark.rs") == "other"


@pytest.mark.parametrize(
    ("percent", "color"),
    [(95.0, "brightgreen"), (90.0, "brightgreen"), (85.0, "green"), (72.0, "yellowgreen")],
)
def test_badge_color_follows_the_shields_thresholds(percent, color):
    assert coverage_report.badge_color(percent) == color
