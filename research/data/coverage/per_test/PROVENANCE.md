# Per-test line coverage

What `measure/per_test_coverage.py` last measured. The data files beside this one are gitignored:
they are one bitset per test, meaningful only against the exact instrumented binary that wrote
them, and `make measure-per-test-coverage` (research/Makefile) regenerates them in about ten
minutes. Query them with `analysis/coverage_sets.py`.

## Last run

- **Measured:** 2026-09-22, against commit `cf60d1ce` with a clean `src/`.
- **Binary:** the library's own test binary, release profile, default features, instrumented by
  `cargo llvm-cov show-env`, built into `target/percov/`. Only this binary: the tests of the
  `src/bin/` tools and of the `stats`/`web` features are not in it.
- **Tests:** 3938 listed. 3924 ran and passed, each in its own process. 14 are `#[ignore]`d and are
  recorded with status `ignored` and no coverage, because selecting one by name runs nothing.
- **Lines:** 31782 instrumented lines outside the fixture stubs - 17258 engine (`src/diff*`,
  `src/code*`), 6558 test harness (`src/test*`), 7966 other. The 11510 lines of the 1220 fixture
  stub files in `src/test/fixtures/` are excluded, since each is reached only by its own fixture.
- **Runtime:** 12 minutes 37 seconds of CPU, 7 tests at a time.

## Validation

The union of the 3924 isolated measurements was compared against one run of the whole suite in a
single process, with the same binary. They agree to within 24 lines of about 28700, none of them in
the engine:

- 1 line only the shared-process run reaches: the fixture cache's hit path in
  `src/test/helper.rs`, which needs a second test in the same process to ask for an already parsed
  pair. In isolation every cache is cold.
- 23 lines only the isolated runs reach: the body of one `src/tui/app.rs` test that fails when the
  whole suite shares a process and passes alone, so the shared run stopped before reaching them.

## First results, engine lines only

- All 3209 fixture tests together reach 9201 engine lines (53.3%). 129 lines are reached by every
  one of them.
- The 1218 `mapping` tests, the ones that run the diff, reach 7907. 1385 lines are reached by every
  mapping test.
- Those 1218 mapping tests have 1109 distinct footprints. The groups of byte-identical footprints
  are small (at most 8) and are all one kind of edit repeated in one repository: version strings,
  commit hashes, whitespace-only changes.
- A greedy set cover reaches 90% of the mapping tests' union with 5 tests, 99% with 22, and 100%
  with 45. One test, `rust_real_logic_change_in_a_huge_75k_node_file`, alone reaches 6243 lines.
- 17 mapping tests reach a line no other mapping test reaches, 201 such lines in all, 128 of them
  from `rust_next_font_imports_generator`.
