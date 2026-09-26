# Provenance

Measured against the human-authored ground-truth fixtures in `src/test/data/diffs/` (NOT the
sampled corpus in `../samples/` - quality is scored on hand-verified mappings, so re-sampling the
corpus does not stale these files). `quality_baseline.txt` is updated only via
`make update-quality-baseline` at the repository root.

`human_mapping_analysis.csv` also defines *which* fixtures are in scope for
`analysis/ambiguity_report.py` (the paper's RA1.1): that script reads the `human_mapping.json`
files directly but restricts itself to the names listed here, so every number it produces
describes one corpus state. Refresh order after adding fixtures: `analyze_human_mappings --csv`, then
`make ambiguity-report`.

## Refresh of 2026-09-26

`optimal_solutions_benchmark.csv` (`benchmark_optimal_solutions --csv`) and
`human_mapping_analysis.csv` (`analyze_human_mappings --csv`) re-run, in that order, alongside a full `benchmark_other` refresh that added
srcDiff, so the paper's comparison, node accuracy, ambiguity and rendering blocks describe one corpus
state again. Annotation had moved on since 2026-09-16: 161 more fixtures in scope, every one of
them a Defects4J unit (274 solved, from 113).

| | fixtures | with a `human_mapping.json` |
|---|---|---|
| `handmade` | 63 | 62 |
| `small` (Curated) | 220 | 220 |
| `full` (Full) | 232 | 232 |
| `stratified` | 491 | 491 |
| `defects4j` | 996 | 274 |
| **total** | **2002** | **1279** |

The paper's scope is **1217** fixtures (1056 before). Over them codediff maps 10,325,792 of
10,333,777 nodes (99.92%), and 7,030,055 of 7,035,541 visible nodes; `paper_variables.py`'s
authored CORPUS block carries these totals.

The trigger was a crash, which is worth knowing about: `benchmark_other_report.py` joins its
per-dataset node columns against this CSV, and a fixture solved after this CSV was written reads
`-` there. The refreshed comparison CSV scored 143 such fixtures, and the report died on
`int('-')`. A crash was the right outcome - the other reading was a comparison section over 1217
fixtures inside a paper whose every other block said 1056.

## Scope

The paper's scope is `_common.PAPER_DATASETS`: every dataset above except `handmade`, which stays
a regression suite. Only fixtures with a `human_mapping.json` are ever scored; the unsolved
Defects4J directories are invisible to every report. Which Defects4J units are solved is how far
annotation has reached, not a draw - do not read a Defects4J figure as an estimate over Defects4J.

## Other files in this directory

- `quality_baseline.csv` - the per-fixture accuracy gate of `make check-quality`, written by
  `make update-quality-baseline` as a projection of the `fixtures` stubs' limits.
- `painting_attribution.csv` - the painting gate of `make check-painting-attribution`, one row per
  fixture and preset, written by `make update-painting-attribution` (the `painting_failure_census`
  test).
- `mismatch_census.csv` - every mismatch against the human mapping, classified by operation,
  reason and node kind, written by the `mismatch_census` test.
- `convention_census.csv` - leaves whose text survived but whose painting differs between
  fixtures, written by the `cross_fixture_convention_census` test.
- `kind_mismatches.csv` - every ground-truth pair whose two nodes differ in kind, written by
  `analyze_human_mappings --kind-mismatches`.
- `kind_invariant_candidates.csv` - the delete+insert leaf pairs the kind invariants would force
  to match, written by `analyze_human_mappings --kind-invariant-cost`.

The three census tests live in `src/test/helper/human_mapping/tests/exploratory.rs`, are
`#[ignore]`d, and run with `cargo test --release --lib --features test-fixtures <name> --
--ignored`.
