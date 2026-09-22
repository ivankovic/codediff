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

import json
import re

import _common
import apted_only_report
import benchmark_other_report
import ci_local
import coverage_report
import coverage_sets
import edit_shape_stats
import numpy as np
import per_test_coverage
import pytest
import yaml

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


def test_shallow_boundary_commits_reads_the_graft_points(tmp_path):
    git_dir = tmp_path / ".git"
    git_dir.mkdir()
    (git_dir / "shallow").write_text("aaa111\nbbb222\n\n")
    assert edit_shape_stats.shallow_boundary_commits(tmp_path) == {"aaa111", "bbb222"}


def test_shallow_boundary_commits_is_empty_for_a_complete_clone(tmp_path):
    # No .git/shallow at all: a full clone has no graft points, and reading it must not raise.
    (tmp_path / ".git").mkdir()
    assert edit_shape_stats.shallow_boundary_commits(tmp_path) == set()


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


def test_matrix_combinations_expands_a_standalone_include_one_entry_per_job():
    """Each entry is its own job. The rule that an include entry merges where it "agrees with
    every axis value" is vacuously true when there are no axes, so without this case all the
    entries fold into one and `${{ matrix.x }}` expands to the last one's values."""
    job = {
        "strategy": {
            "matrix": {
                "include": [
                    {"features": "", "shard": 1},
                    {"features": "stats", "shard": 3},
                ]
            }
        }
    }
    assert ci_local.matrix_combinations(job) == [
        {"features": "", "shard": 1},
        {"features": "stats", "shard": 3},
    ]


def test_matrix_combinations_merges_an_include_into_the_combinations_it_agrees_with():
    job = {
        "strategy": {
            "matrix": {
                "features": ["", "stats"],
                "include": [{"features": "stats", "shard": 3}],
            }
        }
    }
    assert ci_local.matrix_combinations(job) == [
        {"features": ""},
        {"features": "stats", "shard": 3},
    ]


def test_matrix_combinations_adds_an_include_that_matches_no_combination():
    job = {
        "strategy": {
            "matrix": {
                "features": ["", "stats"],
                "include": [{"features": "web", "shard": 4}],
            }
        }
    }
    assert ci_local.matrix_combinations(job)[-1] == {"features": "web", "shard": 4}


def test_matrix_combinations_applies_a_keyless_include_to_every_combination():
    job = {"strategy": {"matrix": {"features": ["", "stats"], "include": [{"shard": 9}]}}}
    assert ci_local.matrix_combinations(job) == [
        {"features": "", "shard": 9},
        {"features": "stats", "shard": 9},
    ]


def test_matrix_combinations_refuses_exclude_rather_than_ignoring_it():
    """Silently dropping `exclude` would run combinations CI does not - the same class of
    mistake as a silently unexpanded `include`."""
    job = {"strategy": {"matrix": {"features": ["", "stats"], "exclude": [{"features": ""}]}}}
    with pytest.raises(RuntimeError):
        ci_local.matrix_combinations(job)


# --- scripts/coverage_report.py ----------------------------------------------------------------


def test_area_of_picks_the_most_specific_prefix():
    assert coverage_report.area_of("src/diff/apted/engine.rs") != "other"
    assert coverage_report.area_of("src/bin/human_solver/main.rs") == "bin/ - dev tools"
    assert coverage_report.area_of("benches/diff_code_benchmark.rs") == "other"


def test_area_of_puts_a_module_root_with_its_own_module():
    """`src/diff.rs` is the engine's root, not an unclassified file - it used to land in `other`,
    where no row printed it and no product total counted it."""
    for module in ("diff", "code", "tui", "stats", "test"):
        directory = coverage_report.area_of(f"src/{module}/whatever.rs")
        assert coverage_report.area_of(f"src/{module}.rs") == directory, module


def test_area_of_gives_top_level_files_their_own_area_not_other():
    """A file directly in `src/` that is nobody's module root is product code with a row of its
    own; `other` stays the signal that something needs adding to AREAS."""
    for path in ("src/main.rs", "src/review.rs", "src/git_configure.rs", "src/lib.rs"):
        assert coverage_report.area_of(path) == coverage_report.TOP_LEVEL, path
    assert coverage_report.area_of("tools/helper.rs") == "other"


@pytest.mark.parametrize(
    ("percent", "color"),
    [(95.0, "brightgreen"), (90.0, "brightgreen"), (85.0, "green"), (72.0, "yellowgreen")],
)
def test_badge_color_follows_the_shields_thresholds(percent, color):
    assert coverage_report.badge_color(percent) == color


# --- the ruff pin, which four files have to agree on ---------------------------------------------


def ruff_pin_in_ci() -> str:
    """The version every `astral-sh/ruff-action` step in ci.yml is pinned to."""
    workflow = yaml.safe_load(ci_local.WORKFLOW.read_text())
    versions = {
        step["with"]["version"]
        for job in workflow["jobs"].values()
        for step in job.get("steps", [])
        if step.get("uses", "").startswith("astral-sh/ruff-action")
    }
    assert len(versions) == 1, f"ci.yml pins ruff to more than one version: {sorted(versions)}"
    return str(versions.pop())


def test_the_ruff_version_named_outside_ci_matches_the_one_ci_pins():
    """`ruff` is installed per-run by CI and by hand everywhere else, so the version CI gates on
    is the only one that decides a push - and the two places that tell a human which to install
    (the root Makefile's `lint-python` failure message and CONTRIBUTING.md) are hand-written copies
    of it. A local ruff of a different version disagrees with the gate, which is exactly the drift
    `scripts/ci_local.py` avoids for the *commands* by reading them out of ci.yml. The version has
    no such reader, so it gets this check instead."""
    pin = ruff_pin_in_ci()
    root = ci_local.REPO_ROOT

    makefile = (root / "Makefile").read_text()
    assert f"RUFF_VERSION := {pin}" in makefile, (
        f"the root Makefile's RUFF_VERSION is not ci.yml's pin ({pin})"
    )

    contributing = (root / "CONTRIBUTING.md").read_text()
    named = set(re.findall(r"ruff@([0-9]+(?:\.[0-9]+)*)", contributing))
    assert named == {pin}, f"CONTRIBUTING.md names ruff@{named or '(none)'}, ci.yml pins {pin}"


# ── distributions_report ────────────────────────────────────────────────────────────────────────


@pytest.mark.parametrize("q", [50, 90, 99])
def test_percentile_of_counts_agrees_with_the_list_percentile(q):
    """The dots on the paper's cumulative curves must land on the numbers its prose prints, and
    those come from `edit_shape_stats.percentile` over the sorted list."""
    import distributions_report

    population = [1, 1, 2, 2, 2, 3, 5, 8, 8, 13, 21, 34]
    values, counts = (
        np.array([1, 2, 3, 5, 8, 13, 21, 34], float),
        np.array([2, 3, 1, 1, 2, 1, 1, 1], float),
    )
    assert distributions_report.percentile_of(values, counts, q) == edit_shape_stats.percentile(
        sorted(population), q
    )


def test_ecdf_is_monotone_and_ends_at_one_hundred():
    import distributions_report

    y = distributions_report.ecdf(np.array([1.0, 2.0, 4.0]), np.array([1.0, 1.0, 2.0]))
    assert list(y) == [25.0, 50.0, 100.0]


def test_rq1_series_keeps_timed_out_pairs_in_the_denominator(tmp_path):
    """A pair the budget killed has no elapsed time but is still a pair attempted: the curve's
    height at the budget is completions over *every* pair, which is what RA2 states."""
    import distributions_report

    csv_path = tmp_path / "g.csv"
    csv_path.write_text(
        "language,status,elapsed_ms\n"
        "Rust,ok,10\nRust,ok,20\nRust,timed_out,\nRust,timed_out,\nRust,out_of_memory,\n"
        "Rust,parse_failed,\nRust,worker_error,\n"
        "YAML,ok,5\n"
    )
    series = distributions_report.rq1_series([csv_path])
    times, total = series[apted_only_report.CODE]
    # Out of memory is a pair attempted and not completed; a parse or harness failure never
    # reached the question and is left out, as in apted_only_report.
    assert list(times) == [10.0, 20.0] and total == 5
    assert series[apted_only_report.CONFIG_DATA][1] == 1


def test_distribution_rows_are_the_whole_population_as_value_counts():
    acc = edit_shape_stats.Accumulator()
    acc.add("c1", "a.rs", 3, 0, 100)  # 3 lines changed, churn 3/100
    acc.add("c1", "b.rs", 1, 2, 50)  # 3 lines changed, churn 3/52
    acc.add("c2", "a.rs", 10, 0, 100)
    rows = list(acc.distribution_rows())
    by = {(r["metric"], r["value"]): r["count"] for r in rows}
    assert by[("lines_per_file", 3)] == 2 and by[("lines_per_file", 10)] == 1
    assert by[("lines_per_commit", 6)] == 1 and by[("lines_per_commit", 10)] == 1
    assert by[("files_per_commit", 2)] == 1 and by[("files_per_commit", 1)] == 1
    churn = {v: c for (m, v), c in by.items() if m == "churn_permille"}
    assert sum(churn.values()) == 3 and 30 in churn and 100 in churn


# ── list_code_edits ─────────────────────────────────────────────────────────────────────────────


@pytest.mark.parametrize(
    ("numstat_path", "old", "new"),
    [
        ("src/main.rs", "src/main.rs", "src/main.rs"),
        ("static/{Dockerfile-10 => Dockerfile-11}", "static/Dockerfile-10", "static/Dockerfile-11"),
        ("src/gui/{a.ml => b.ml}", "src/gui/a.ml", "src/gui/b.ml"),
        ("{old => new}/lib/x.py", "old/lib/x.py", "new/lib/x.py"),
        ("a/{b => c}/d/{e => f}.go", "a/b/d/e.go", "a/c/d/f.go"),
        ("before.rs => after.rs", "before.rs", "after.rs"),
        # A move into a new directory component: the left side of the group is empty, and git's
        # path has no doubled slash where it was.
        (
            "Thirdparty/ffmpeg/{ => ffmpeg}/ffmpeg-6.0/x.h",
            "Thirdparty/ffmpeg/ffmpeg-6.0/x.h",
            "Thirdparty/ffmpeg/ffmpeg/ffmpeg-6.0/x.h",
        ),
        ("src/{old => }/mod.rs", "src/old/mod.rs", "src/mod.rs"),
    ],
)
def test_rename_sides_recovers_both_paths_of_a_numstat_rename(numstat_path, old, new):
    import list_code_edits

    assert list_code_edits.rename_sides(numstat_path) == (old, new)


# --- measure/per_test_coverage.py + analysis/coverage_sets.py ---------------------------------------


def test_per_test_coverage_classifies_module_roots_with_their_modules():
    """Every "engine" number the set tool prints rests on this: `src/diff.rs` is the engine's own
    root, and missing it would drop 581 lines of the engine from every set."""
    assert per_test_coverage.area_of("src/diff.rs") == "engine"
    assert per_test_coverage.area_of("src/diff/apted/engine.rs") == "engine"
    assert per_test_coverage.area_of("src/code.rs") == "engine"
    assert per_test_coverage.area_of("src/test/helper/human_mapping.rs") == "harness"
    assert per_test_coverage.area_of("src/test.rs") == "harness"
    assert per_test_coverage.area_of("src/tui/app.rs") == "other"
    assert per_test_coverage.area_of("src/differ.rs") == "other", "a prefix, not a module"


def test_coverage_sets_groups_a_fixtures_tests_but_leaves_other_tests_alone():
    assert (
        coverage_sets.fixture_of("test::fixtures::handmade::rust_add_if::mapping")
        == "test::fixtures::handmade::rust_add_if"
    )
    assert coverage_sets.fixture_of("diff::tests::test_compute_metadata") == (
        "diff::tests::test_compute_metadata"
    )


def write_per_test_data(root, lines, tests):
    """A per_test directory by hand: `lines` is [(area, file, line)], `tests` is
    {name: [covered line positions]} - the same shape measure/per_test_coverage.py writes."""
    (root / "bits").mkdir()
    with open(root / "lines.tsv", "w") as handle:
        handle.write("index\tarea\tfile\tline\n")
        for index, (area, file, line) in enumerate(lines):
            handle.write(f"{index}\t{area}\t{file}\t{line}\n")
    width = (len(lines) + 7) // 8
    with open(root / "records.jsonl", "w") as handle:
        for number, (name, positions) in enumerate(tests.items()):
            bits = 0
            covered = {}
            for position in positions:
                bits |= 1 << position
                area = lines[position][0]
                covered[area] = covered.get(area, 0) + 1
            (root / "bits" / f"k{number}").write_bytes(bits.to_bytes(width, "little"))
            record = {"name": name, "status": "ok", "bits": f"k{number}", "covered": covered}
            handle.write(json.dumps(record) + "\n")


LINES = [
    ("engine", "src/diff.rs", 1),
    ("engine", "src/diff.rs", 2),
    ("engine", "src/diff.rs", 3),
    ("harness", "src/test.rs", 1),
    ("engine", "src/code.rs", 7),
]


def test_coverage_sets_masks_to_the_area_and_combines_sets(tmp_path):
    write_per_test_data(
        tmp_path,
        LINES,
        {
            "test::fixtures::small::a::mapping": [0, 1, 3],
            "test::fixtures::small::a::painting": [1, 2],
            "test::fixtures::small::b::mapping": [1, 4],
        },
    )
    coverage = coverage_sets.Coverage(tmp_path, "engine", "test")
    sets = coverage.select(None)
    # Line 3 is the harness: masked out of every set before any operation.
    assert sets["test::fixtures::small::a::mapping"] == 0b00011
    assert coverage_sets.union(sets.values()) == 0b10111
    assert coverage_sets.intersection(sets.values()) == 0b00010

    reach = coverage.reach_counts(sets.values())
    assert list(reach) == [1, 3, 1, 0, 1]
    assert coverage.bits_where(reach == 1) == 0b10101
    assert coverage.describe(0b00111, 0) == ["src/diff.rs:1-3"]

    fixtures = coverage_sets.Coverage(tmp_path, "engine", "fixture").select(None)
    assert fixtures == {"test::fixtures::small::a": 0b00111, "test::fixtures::small::b": 0b10010}


def test_coverage_sets_refuses_bits_that_do_not_match_their_record(tmp_path):
    """A bits file from another run read against this lines.tsv would silently answer every
    question wrongly; the per-area counts the run recorded are what catch it."""
    write_per_test_data(tmp_path, LINES, {"t::one": [0, 1]})
    (tmp_path / "bits" / "k0").write_bytes((0b111).to_bytes(1, "little"))
    with pytest.raises(SystemExit):
        coverage_sets.Coverage(tmp_path, "engine", "test")
