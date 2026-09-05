# Code Health Review — 2026-09-05

Scope: dead code, duplication, ease of understanding, runtime performance, testing. Whole
repository (Rust crate, `src/bin/` tools, `research/`, `scripts/`, `assets/mapping_site`, both
Makefiles, CI). Method: four independent surveys (dead code, duplication/readability, Python and
build, tests and hot path), a window-hash duplicate-block scan over `src/` and `research/`, a timed
run of the full release suite, and a manual re-check of every claim listed below. Claims that did
not survive the re-check were dropped (for example, `OverlayTheme::to_palette` and
`DiffViewer::scroll_view` were reported dead and are not; the three APTED fuzz tests were guessed
to dominate wall-clock and take 0.3s each). Everything below was verified against the tree at this
date. Line numbers are as of commit `ebcf904`.

The previous review (2026-07-06, re-measured 2026-09-03) follows this section. Its open items are
re-verified here rather than repeated: 1.7, 1.8, 1.9 (`was_*` helpers), §3 `DiffPass` trait, and
§5 items 1-4 are still open; 1.9's `ranges` arm de-duplication and the `blob_content` sharing are
done; the §4 typos are fixed except `symetric` (×3).

## Status

- **2026-09-05, section 1** - all six defects fixed: `benchmark-ablation` path, `build: test`
  is handled in section 2, `code_percentiles.csv` path, `benchmark_other`'s provenance copy
  replaced by `helper::sample_provenance` (its `language` column now comes from the fixture's
  detected language, which is the same detector `sample.csv` recorded), `dataset.sh`'s project
  filter, R/Scala in `LANGUAGE_CATEGORY`, `[[bench]]` gating.
- **2026-09-05, section 2** - done: `build` no longer depends on `test`; the seven
  feature-only dependencies are gated (`tempfile`/`regex` also as dev-dependencies for the
  `cfg(test)` builds of `src/test/`); one `FEATURES ?= stats` for every local release-binary
  target, CI's quality job overrides it with `test-fixtures`; `BENCH_QUALITY`/`extract-ms`
  replace the triplicated invocation; `.PHONY` in the root Makefile; ruff config moved to a root
  `ruff.toml` and `make lint-python`, the pre-push hook and CI all lint `research scripts
  assets`; CI's JS job calls `make test-mapping-site-js`. Left open: `research/Makefile`'s own
  items (no `.PHONY`, `matching-reasons-report`'s benchmark prerequisite, the
  `measure-apted-budget` blocks) - that file had uncommitted edits at the time.
- **2026-09-05, section 7 (wall-clock)** - done: the two corpus invariant tests are one
  streamed, multi-threaded pass (147s of CPU across two tests, now 16s wall on one); the
  corpus-loader test checks the directory walk instead of parsing the corpus (43s to 0.01s);
  the seeding test loads its one fixture (41s to 2.4s); `painting()` diffs each fixture once for
  both presets (`compare_painting_with_diff`); `code_pair_from_dir_without_metadata` gives the
  inventory and the `human_solver` picker scan a tree without the hashes and sketches they never
  read (measured 2026-09-05: parsing the corpus is 3.7s, metadata another 12s), which also
  speeds up the picker's `o` scan in the tool itself. Still over 5s: the inventory test (31s,
  now dominated by reading 1.4GB of `human_mapping.json`), the picker scan test (12.6s), the
  invariants pass (16s), and the largest fixtures' own mapping tests. Left open: `NodeCache`
  rebuilt outside `diff_code` (the transmuted `'static` cache would have to travel in `Diff`).
- **2026-09-05, section 3** - done: `DiffMode`, `--exact`'s effect and the `fallback_used`
  field are gone (`PendingDiff::finish()` takes no mode; `--exact` stays as a hidden no-op for
  one release; JSON output's field is `large_residual`, which is what it always measured, and
  the TUI never read it at all); the `Suspend`/`Resume` action chain, `Ui::suspend`/`resume` and
  with them the `signal-hook` dependency; `ASTMappingOperation::{DoNothing, Move}` and
  `COST_MOVE`; `Diff::is_node_mapped`; `ScreenRow`; `Sketch::is_exact` and
  `PendingDiff::unmatched_counts` are `#[cfg(test)]`; `zhang_shasha.rs`, `Algorithm::ZhangShasha`
  and the indexer's `pre_to_post`/`keyroots` are compiled only for the oracle tests; the three
  `debug_dump_*` tests are `#[ignore]`d. The phase-6 history comment moved to
  `src/diff/TODO.md`. Left open: `stats::is_generated` (the survey was wrong - `expand_stats`
  calls it); `TextRange::intersects` (harmless API); `ASTMappingReason::OptimalIDU` (its
  `OptIDU` column is read by `matching_reasons_report.py` and lives in the checked-in benchmark
  CSV, so it goes with the next CSV regeneration, not alone); the 22 glob re-exports.
- **2026-09-05, section 5 (first bullet)** - done: the twelve contradicting or dangling
  comments listed there are corrected (phase numbering, `final_apted`, the deleted constant, the
  Myers `FLAT_MIN_CHILDREN` wording, both misattached doc comments, the nonexistent
  `compute_diff_interactive`, the broken intra-doc links, the `human_solver.rs` paths, the
  deleted `solve_bottom_up_expansion` described as live, `apted/mod.rs`'s "three files", the
  `apted/common.rs` citations for items that moved into its submodules, and the `symetric`/
  `programms` typos). Section 5's remaining bullets (history narrative, module names, boolean
  parameters, `is_semantically_structural`, doc style) are still open apart from the phase-6
  history that went to `src/diff/TODO.md` with section 3.
- **2026-09-05, section 4 (first cut)** - done: `ASTMapping::{identical, matched_not_identical,
  updated, deleted, inserted}` replace the 31 standard-shape literals (the computed-cost ones
  stay written out); `text_range::floor_char_boundary` is the one home for the byte-column clamp
  (headless, the code viewer widget, the mapping site, the fake diff tool) and
  `text_range::paint_row_len` for the trimmed paint extent; `OverlayPalette::background_for` is
  the one operation-to-colour table for the ratatui renderers. `make check-quality`: 0 regressed,
  0 improved. Still open: items 2 (`PassCtx`, ~50 call sites incl. tests), 4-9, 10-11.
- **2026-09-05, sections 2/3/6-7 (research)** - done: `research/analysis/_common.py` holds
  the CSV readers, `latex_number`, `REPO_ROOT` and the chart chrome (7+3+2+5 copies removed;
  every fragment the report targets write was checked byte-identical before and after);
  `optimal_solutions_benchmark_report.py` and its five unreferenced PNGs deleted;
  `commit_stats.py` no longer plots the six hardcoded-zero churn columns; `research/Makefile`
  has `.PHONY`, `matching-reasons-report` re-renders from disk, `measure-apted-budget` is one
  `foreach` over four language-group variables and delegates to `build`; pytest (28 tests over
  the pure functions in `research/analysis/` and `scripts/`) runs as `make test-python`, in
  `make test` and in CI. The committed `plots/*.tex` fragments were stale against the committed
  scripts and data (e.g. `ShapeFixtures` 512 vs 597) and are regenerated.
- **2026-09-05, section 4 item 2** - done: `diff::PassCtx` (both sides, the `NodeCache`, both
  sides' metadata resolved once) is the one signature for all fourteen passes,
  `fn solve(ctx: &PassCtx, diff: &mut ASTDiff)`; the five `_`-prefixed parameters and the
  per-pass `metadata_of` calls at entry are gone (13 pipeline call sites, 32 test call sites).
  The ordered `PIPELINE` constant was not added: the phases are interleaved with config gates
  and two non-pass calls, so the list in `pending_with_config`'s comment stays the record.
- **2026-09-05, section 4 items 4 and 9 (part)** - done: the apted before/after mirror pairs
  share one body each - `emit_subtree(Side, ..)` behind `emit_before/after_subtree`,
  `subtree_cost(Side, ..)` behind `subtree_del/ins_cost`, and a `SideDecision` trait over the
  two decision enums behind `match_target`/`collect_side_subtree_targets` (the four
  `before_*`/`after_*` names stay as one-line wrappers for their callers). `human_solver`'s
  private `Side` twin is the `human_mapping::Side` re-exported. Its five small mirror pairs
  (`algo_status_*`, `algo_reason_*`, `algo_disagrees_*`, `advance_*_to_next_unmarked`,
  `clear_*_descendants`) are still open.
- **2026-09-05, section 4 item 6** - done: `diff/text.rs ranges` is 57 lines over a
  `RangeWalk` whose methods are the former arms (`identical_or_move`, `update_ranges`,
  `own_content_update_ranges`, `whole_content_prune` + `own_gap_ranges`, `placed`, `push`),
  with the measurement narratives carried over as each method's doc comment; `classify_node`/
  `NodeChange` is the one operation-to-visible-change classification, used by both `ranges` and
  `summary::scan`.
- **2026-09-05, section 5 (boolean parameters)** - done for the `RenderOptions` chain:
  `TextDiff::from_with_options`, `ranges`, `compute_diff_with_options` (was
  `compute_diff_with_update_style`) and `assemble_diff_session_data` take a `RenderOptions`
  instead of the two construction-time flags forwarded positionally through three levels;
  `TextDiff::from` is `from_with_options(.., RenderOptions::FULL)`, whose two flags are exactly
  the legacy literals it hardcoded; `from_with_update_style` had no callers. Left as they are:
  `intra_node_update_ranges`'s private three bools, `exit_code_for(bool, bool, bool)` (three
  unrelated facts, pinned by its tests) and `headless::render_side`'s two.
- **2026-09-05, section 5 (`is_semantically_structural`)** - done: `named_child_text` replaces
  the twenty arms of the "named child of an accepted kind" shape with one line each (485 to 439
  lines). The rest is the wide-table case the review allowed for: 48 arms that genuinely differ
  (Rust `impl_item`, Go receivers and specs, C declarators, ...), most carrying the measurement
  that put them there. `make check-quality`: 0 regressed, 0 improved.
- **2026-09-05/06, section 5 (history narrative)** - done for the four blocks that were pure
  chronology: `solve_greedy_anchor_blocks`' all-pairs postmortem, `solve_identical_diagnostic_
  statements`' paragraphs about a deleted pass, `myers.rs`'s two tried-and-reverted accounts and
  `residual.rs`'s restatement each keep their invariant in source and point at "Design history
  moved out of source" in `src/diff/TODO.md`, which now holds the dates and fixtures. `nodes.rs`
  2064-2105 (a table of formerly-wrong kind names) stays: it is a verification record of the
  current table, not history.
- **2026-09-06, section 5 (module names)** - done for the cheap ones: `src/metadata.rs` is
  `src/anomalous_paths.rs` (six call sites), and the crate's two `mod.rs` files are the sibling
  files `diff/apted.rs` and `tui/widgets.rs` like every other module. Left open: splitting the
  kind taxonomy out of `diff/nodes.rs`, `apted/common.rs`'s glob re-export tail (and the 22
  `#[allow(unused_imports)]` globs behind it), and the `CodeViewer`/`CodeViewerWidget` naming.
- **2026-09-06, section 3 (glob re-exports)** - done: all 22 `#[allow(unused_imports)]` are
  gone. Only four warnings ever hid behind them - `pub use x::*` tails re-exporting nothing
  `pub` - which are `pub(crate) use` now; the `use super::*`/`use crate::*` globs themselves
  warn on nothing, so the lint is armed in both subsystems without touching them.
- **2026-09-06, section 4 item 9 (rest)** - done: `human_solver`'s five mirror pairs are
  `algo_status`, `algo_reason`, `algo_disagrees`, `advance_side_to_next_unmarked` and
  `clear_descendants`, each taking a `Side`; `render.rs`'s three `match side` dispatches over
  them are direct calls. The asymmetry that mattered (`Some(0)` is Deleted on one side and
  Inserted on the other) is the one `match` left inside `algo_status`.
- **Deliberately left (2026-09-06)**: splitting the kind taxonomy out of `diff/nodes.rs`. The
  file is three mutators (100 lines) over a 2,000-line table of node-kind and language
  predicates, and the 100 `nodes::is_comment`-style call sites read correctly as they are; a
  rename buys the file a better name at the cost of touching all of them. Revisit if the table
  grows another concern. Also left: `CodeViewer` vs `CodeViewerWidget`/`CodeViewerState`.
  What remains of the review is section 6 (performance): profile with `make benchmark-speed`
  (buildable again since section 1), then the metadata arena.
- **2026-09-06, section 6 (profile + first fixes)** - `perf` is locked down here
  (`perf_event_paranoid` 4), so the profile is callgrind on the product binary diffing the 75k-node
  Rust fixture, plus a per-step corpus timing. Findings: APTED's `spf_path` is 26% self (expected);
  **`Node::parent()` was 12% of all instructions** - tree-sitter's parent lookup descends from
  the root, and `solve_wrap_growth` asked for a parent once per shifted internal node (542k calls
  in `finish`); `sort_deepest_first` did two hash lookups per comparison, 6.7%; metadata is 3.6x
  the parse on that file (`compute_ast_metadata` 24.7% vs `ts_parser_parse` 6.8%), spread over
  the eight walks' per-node cursor creation, `Vec<Node>` collects and hash-map inserts rather than
  any single step - the leaf-only `text` change (below) moved the corpus number 11.85s to 11.64s,
  so the copy was not the cost. Fixed: parent lookups in `solve_wrap_growth`,
  `solve_heritage_clause_growth` and the comment-only scan go through `node_to_parent`;
  `sort_deepest_first` uses `sort_by_cached_key`; `ASTNodeMetadata::text` is stored for leaves
  only (every consumer compares leaves). Product binary on the 75k-node fixture: 2.78s to 2.41s;
  on the 900KB JSON fixture 3.08s to 2.94s. Next on this list: the arena (items 6.2-6.4).
- **Runtime finding from the same measurement**: over the corpus, AST metadata costs about three
  times the tree-sitter parse, and the parse plus metadata (15.5s) is more than half the diff
  itself (28s per the quality baseline). Section 6's items 1-3 are that cost.

## Headline numbers

| Measure | Value |
|---|---|
| Rust source | 710 files, 104k lines (fixture tables ~30k of that) |
| Tracked Python | 7.1k lines (`research/analysis` 5.9k, `scripts/` 0.5k) |
| Full release suite | 1736 tests, 139s wall, 491s CPU, 0 failures |
| Six tests over 30s each | 321s CPU, 65% of the suite |
| Duplicate blocks ≥12 code lines in `src/` | 109 runs, ~1.6k lines, mostly test boilerplate |
| Compiler / clippy warnings | 0 (CI enforces `-D warnings`) |

## 1. Correctness defects found along the way (fix first, trivial)

- **`make benchmark-ablation` is broken.** `scripts/ablation_study.sh:32` does
  `cd "$(dirname "$0")/../.."`, correct when the script lived in `research/measure/`; since the
  move to `scripts/` it lands in the parent of the repo, so `cargo build` fails. Fix: one `..`.
- **`research/analysis/file_stats.py:466`** writes `code_percentiles.csv` to the working
  directory, not to `data/corpus_stats/` which `data/README.md` names as Table 1's source.
- **`bin/benchmark_other.rs:1248 sample_provenance`** is a cwd-relative copy of
  `test/helper.rs:847`; run from anywhere but the repo root it silently blanks the
  `repository/commit/path` columns of the accuracy CSV. Delete the copy, call the helper.
- **`research/sampling/dataset.sh:36`** tests `$dataset`, which is never set, so the
  `-p/--project` filter skips every repository.
- **`research/analysis/apted_only_report.py:149 LANGUAGE_CATEGORY`** lacks R and Scala while
  `measure-apted-budget` group 4 feeds `sampled_code_pairs_{r,scala}.csv`; `categories_of`
  exits on an unclassified language. Latent until the next re-measurement.
- **`benches/diff_code_benchmark.rs`** uses `codediff::test`, which is gated behind
  `test-fixtures`; the `[[bench]]` entry has no `required-features`, so a bare `cargo bench` (and
  rust-analyzer) fails to build it.

## 2. Build, dependencies, CI

- **`Makefile:79 build: test`** makes the five research measurement targets that depend on
  `build` run the whole release suite first. `deploy-checks` already gates explicitly.
- **Seven dependencies are unconditional but used only behind a feature**: `tempfile`
  (tests and the `test-fixtures` module only), `regex` (`stats` + `test-fixtures`), `futures`,
  `tracing`, `tracing-subscriber`, `tracing-error` (`tui` only), `walkdir` (`stats` only). A
  `default-features = false` library consumer still compiles all of them. Gate them; `tempfile`
  becomes a dev-dependency plus `test-fixtures`.
- **Three feature sets share one `target/release`**, re-linking the fat-LTO binary whenever a
  developer alternates `make build` (`stats`), `make check-quality` (`test-fixtures`) and
  `make test` (default). One `FEATURES := stats` for every local release target fixes it (CI's
  matrix is unaffected).
- **Python lint scope drifts three ways**: CI lints `research scripts`, `make lint-python` lints
  `research` only, the pre-push hook says `research/`, and `scripts/` has no ruff config so it
  gets ~415 default rules instead of research's pinned ~138. `assets/bdiff_driver.py` is linted
  nowhere. One root `ruff.toml`, one `make lint-python`, CI calls it.
- `ci.yml:82-83` lists the two JS test files by hand although `make test-mapping-site-js` exists
  for exactly the reason the pre-push hook documents. No `.PHONY` in either Makefile.
- `check-quality` / `update-quality-baseline` / `benchmark-quality` triplicate the benchmark
  invocation and the `ms/fixture` extraction.

## 3. Dead code

Verified by grep for constructions and call sites, not by the compiler (which is clean; the
`#[allow(unused_imports)]` glob re-exports below are why it cannot see this layer).

- **`DiffMode` / `--exact` / `fallback_used`**: `PendingDiff::finish(_mode)` ignores its
  argument (`diff.rs:410`) and says so; `DiffMode` is still threaded through 87 sites in eight
  files, `--exact` is documented as a deprecated no-op, and the JSON field `fallback_used` is
  documented as "whether the guard substituted a cheaper fallback" while the fallback is
  unconditional (`diff.rs:499`), so the field is false by construction. Remove the enum, the
  flag (or keep it hidden for script compatibility) and the field, ~150 lines. Visible CLI/JSON
  surface, but the values are already meaningless.
- **`Action::Suspend` chain**: never constructed. Dead downstream: `App::should_suspend`,
  `app.rs:271-275`, `Action::Resume` (only produced inside that block), `Ui::suspend`/`resume`.
- **`ASTMappingOperation::DoNothing` and `::Move`** are never constructed (every `::Move` in
  the tree is `TextOperation::Move`); `COST_MOVE` goes with them.
- `Diff::is_node_mapped` (0 refs), `stats::is_generated` + `AUTO_GENERATED_RE` (test-only),
  `Sketch::is_exact` (test-only), `PendingDiff::unmatched_counts` (one assert message),
  `TextRange::intersects` (19 refs, all tests), `ScreenRow` newtype (never used).
- `Algorithm::ZhangShasha` and `apted/zhang_shasha.rs` are compiled into the release binary
  purely as a test oracle; `ASTMappingReason::OptimalIDU` outlived `optimal_iud.rs` (deleted
  2026-09-03) and is constructed only in tests. Gate both with `#[cfg(test)]`.
- **22 `#[allow(unused_imports)]` glob imports/re-exports** from the 2026-09-03 module splits
  (`apted/common/*.rs:23`, `apted/common.rs:1137-1150`, `human_solver/*.rs:22`,
  `human_solver/main.rs:224-235`) plus 158 `pub(crate)` items with zero external references are
  what keep rustc's dead-code lint blind in those two subsystems. Explicit imports and private
  visibility would let the compiler do the next survey.
- Three print-only tests that cannot fail: `apted/common/tests.rs:776,790,806 debug_dump_*`
  (only `eprintln!`); mark `#[ignore = "debug dump"]` like their sibling at `:452`.
- Research: `research/analysis/optimal_solutions_benchmark_report.py` (357 lines) and its five
  `optimal_solutions_*.png` have zero references from any Makefile, README, CI or `main.tex`.
  `commit_stats.py:116-256` plots columns `commit_stats.rs` writes as hardcoded zeros.
  `sample-pairs`, `measure-code-pair-diffs` and `process_gentoo_package_list.sh` have no readers.
  ~60 generated LaTeX macros are no longer referenced by `main.tex`.

## 4. Duplication

Ranked by lines saved times drift risk. "Algorithm" marks changes that touch the mapping and must
be checked against `make check-quality`; everything else is rendering, tooling or tests.

1. **Byte-column span walk with the char-boundary clamp, four copies**: `tui/headless.rs:250-301`,
   `tui/widgets/code_viewer.rs:217-270`, `bin/generate_mapping_site.rs:1201-1300`,
   `bin/fake_diff_tool.rs:216-229`. The same byte-vs-char defect was found and fixed separately
   in the two product copies. One `text_range::floor_char_boundary` plus a `segment_runs`
   iterator. Also per renderer: trailing-whitespace row length (×5) and `TextOperation` to
   colour/class (×5).
2. **The 15 `solve_*` passes share an implicit `(before, after, node_cache, diff)` signature that
   five ignore** (`_node_cache` ×4, `_before, _after` ×1); `metadata_of` is re-called at every
   entry (28 calls, 12 in `hash_tree_matching.rs`; cheap borrows, but noise), and six repeat the
   `let Some(ast) = ... else return` guard. A `PassCtx` built once in `pending_with_config` and a
   `const PIPELINE` list put the ordering hazards the memory notes keep tripping on in one place.
   Mechanical; algorithm-neutral.
3. **`ASTMapping { cost, operation, reason }` literal at 45 sites**. Constructors
   (`identical`, `delete`, `insert`, `update`, `matched`) make the cost/operation pairing
   un-mistypable. Zero risk.
4. **apted before/after mirror pairs** (REVIEW 1.7, still open): `emit_before/after_subtree`,
   `subtree_del/ins_cost`, `before/after_match_target`, `collect_before/after_subtree_targets`,
   ~130 lines to ~65 via a `SideCtx`. Algorithm; pure parameterisation.
5. **Lockstep "descend adding Identical mappings" exists four times**: `nodes.rs:35-66`,
   `hash_tree_matching.rs:240-286`, `solve_nested_condition_collapse.rs:303-340`,
   `solve_moved_subtrees.rs:320-352`. Algorithm; identical output if each site keeps its pairer.
6. **`diff/text.rs:372 ranges`** (522 lines by brace count, nesting 10) and
   `text/summary.rs:325-378 scan` re-implement the same operation-to-visible-change
   classification. Split into `move_or_identical`, `own_gap_ranges`, `update_ranges_for`, a
   `RangeAccumulator`, and one shared visitor. Rendering only.
7. **`bin/apted_only_benchmark.rs` vs `bin/benchmark_diff_pairs.rs`**: `read_pairs_csv`,
   `open_repo`, `blob_content` byte-identical, `main()` skeleton identical, ~200 lines. Share the
   plumbing only; the CSV writers are research output and must stay byte-identical.
8. **`test/helper/human_mapping.rs`**: mirror pairs by function name (×5), `check_entry` 157
   lines, `check_group_entry` 226 lines, `compute_mismatches*` as an eight-function suffix matrix
   (`:2443-3089`), `node_kind_for_id` does a full DFS per mismatch though `node_info[id].kind`
   exists. `codediff_line_mismatches` is forked in `bin/benchmark_other.rs:485-509` (published
   metric; must stay byte-identical).
9. **human_solver** (REVIEW 1.8, still open): five mirror pairs, `render.rs:31 Side` duplicating
   `human_mapping.rs:1353 Side`, four picker renderers sharing a byte-identical 15-line header,
   13 functions taking the same seven `(before_flat, after_flat, ..., caches)` parameters.
10. **Research Python**: `read_rows` ×7, `pct` ×5, `latex_number` ×3, `repo_root` ×4,
    `\newcommand` fragment writers ×8, chart-chrome constants ×5, sixteen identical
    savefig/close/print tails. `percentile_report.py` already proves a shared module works; add
    `research/analysis/_common.py`. Language list enumerated five times across Makefile, shell
    and Python. `write_bucket_table` / `write_node_bucket_table` ~110 lines apart by one param.
11. Smaller: `positional_key_before/after` (45 lines), two identical sweeps in
    `solve_unresolved_nodes.rs:60-96`, shared prologue of `solve_heritage_clause_growth` and
    `solve_wrap_growth`, `popup_area` ×6 in the TUI dialogs, scroll-into-view ×3 and
    centre-in-window ×6, `ASTMetadata` lookup idioms (`subtree_size` unwrap ×29, stack-DFS ×37),
    the six-step panel draw run three times in `components/diff_viewer.rs:880-1012`.

## 5. Ease of understanding

- **Comments that contradict `diff.rs`**: `solve_moved_subtrees.rs:19` "the final pass" and
  `diff.rs:511-512` "Phase 7 ... dead last" (phases 8-10 follow); "seven-phase pipeline" in three
  pass docs vs "now runs ten"; `final_apted` cited 6× and never existed under that name;
  `RESIDUAL_SEGMENT_MAX_TOTAL_SIZE` cited after deletion; `resolve_forest`'s and
  `human_mapping_cost`'s doc comments are attached to the wrong item (no blank line);
  `compute_diff_interactive` cited 3×, does not exist; broken intra-doc links
  `painting_for_mode` / `ranges_for_mode`; `src/bin/human_solver.rs` referenced as a file 14×
  including five lines in `Cargo.toml`.
- **History narrated in source**: `diff.rs:420-467` (48 lines of measurements and merge
  receipts), `nodes.rs:2064-2105`, `residual.rs:323-350`, `myers.rs:31-67` and `:559-594` (a
  comment refuting an older comment), `solve_greedy_anchor_blocks.rs:22-86`,
  `solve_identical_diagnostic_statements.rs:24-59` (two paragraphs on a deleted pass). `diff.rs`
  is 42% comment lines; `Cargo.toml` is 30% prose. Three `TODO.md` files (289K, 178K, 36K) hold
  the same investigations unevenly. Keep the invariant sentence in source, move the narrative to
  one dated TODO.md.
- **Module names**: `diff/nodes.rs` is 100 lines of diff mutators plus a 2000-line kind/language
  taxonomy; `src/metadata.rs` (anomalous path list) collides with `code/metadata.rs`;
  `apted/common.rs` is a facade whose tail glob-re-exports five submodules; two `mod.rs` against
  a sibling-file convention everywhere else; `CodeViewer` vs `CodeViewerWidget`/`CodeViewerState`.
- **Boolean parameter soup** around `RenderOptions`: `ranges(.., source_is_before,
  whole_pair_updates, paint_reindent_only_moves)`, `intra_node_update_ranges` (3 bools),
  `exit_code_for(bool, bool, bool)`, the same two flags forwarded positionally through three
  levels of `tui/app.rs`. Pass the struct; use `Side`.
- `nodes.rs:502-987 is_semantically_structural` (485 lines): ~30 of 68 arms are the same
  four-line "named child of accepted kind" closure. A `named_child_text` helper halves it.
- Doc style split: 109 `/** */` blocks in 26 files vs ~1940 `///`.

## 6. Runtime performance

Current corpus latency (`research/data/quality/quality_baseline.csv`, 597 fixtures): p50 4.1ms,
p90 106ms, p99 722ms, max 1667ms. Items 1-4 form one refactor; profile before starting it
(`make benchmark-speed` after fixing the `[[bench]]` gating above).

1. **Every node stores its whole subtree text as an owned `String`** (`code.rs:283-287`,
   filled at `code/metadata.rs:189-193`), so metadata is O(bytes × depth) heap per side, and
   `kind: String` copies a `&'static str` per node.
2. **Nine node-id-keyed `FxHashMap`s** (`code.rs:409-520`) where a `Vec` indexed by the
   `preorder_index` that already exists would do; `NodeCache` is a tenth, built by another walk.
3. **Eight full-tree walks per side** in `metadata.rs:99-110`, each allocating a cursor per node
   and several collecting children into a `Vec` per node.
4. **APTED's inner cost evaluation does hash lookups and `String` compares**: `engine.rs:398
   vnode` looks up `node_info` per `vren/vdel/vins`; `common.rs:92 ren` compares `kind` and
   `text` strings; `resolve.rs:152-185 adjust` adds an `is_ancestor_or_self` parent-chain walk
   with a lookup per hop, per pruned target. Precomputed per-preorder arrays (kind id, text hash,
   `[pre_lo, pre_hi]`) make all of it O(1). Needs profiling to size, but it is the O(n·m) loop.
5. **Myers keeps a full `V` snapshot per `d`** (`myers.rs:116-117`): O(d²) memory,
   `PLAIN_TEXT_MAX_EDIT = 10_000` allows ~1.6GB for two large dissimilar plain-text files.
6. `slots.rs:884-908 compute_pruned_targets` clones its memoised `Vec` at every ancestor level.
7. `std::HashMap`/`HashSet` (SipHash) in `apted/common.rs:638-652`, `hash_tree_matching.rs`,
   `grouped_greedy_matcher.rs`, four `solve_*` passes, while the rest of the crate is Fx. Purely
   mechanical.
8. Per-node `String` keys (`scope.join("::")`) in `solve_syntax_aware_matching.rs:298-306`;
   two `String`s per call in `is_semantically_structural`; six `Vec`s per node in
   `code/hash.rs:129-152`; `TextDiff::all` clones the whole range `Vec`; sort keys doing four
   lookups per comparison in `slots.rs:700-710`.
- Checked and fine: no `Regex::new` in the hot path; no `Vec<Box<_>>`; the `metadata_of` repeats
  are `Cow::Borrowed`.

## 7. Testing

CONTRIBUTING says per-file unit tests must run under 1s and fixture tests under 5s. The suite has
37 tests over 1s and 6 over 30s. Measured (release, nextest, one process per test):

| CPU s | Test | Why |
|---|---|---|
| 73.8 | `text_range::corpus_position_invariants::corpus_ranges_are_addressable...` | diffs all 597 fixtures |
| 72.7 | `text_range::corpus_position_invariants::every_range_the_corpus_produces...` | diffs all 597 again |
| 61.2 | `diff_inventory::tests::every_fixture_in_the_corpus_produces_a_row` | diffs all 597 a third time |
| 42.9 | `test::helper::tests::test_handmade_test_code_pairs_returns_all_diffs` | parses the corpus to check three keys exist |
| 40.9 | `human_solver::tests::seeding_refuses_a_pair_whose_codediff_ranges_overlap` | parses the corpus to fetch one fixture |
| 30.0 | `human_solver::tests::open_diff_picker_f_computes_the_unmarked_map_lazily...` | scans the corpus |

- The two `text_range` invariants can run inside the per-fixture harness (which already computes
  the diff, in parallel across processes) at zero extra cost; the inventory test can sample; the
  `returns_all_diffs` test needs directory names, not parsed trees; the seeding test needs
  `handmade_test_code_pair(name)`, which exists. That alone removes ~300s of 491s CPU.
- **`painting()` runs the diff twice** (`human_mapping.rs:941-965` loops presets and
  `compare_painting` re-diffs per preset), ×298 tests. Diff once, render twice.
- **`NodeCache` is rebuilt 2-4× per fixture test** because `diff_code` builds one internally and
  the harness builds another; nextest's process-per-test also defeats the two `OnceLock` corpus
  caches, so the five largest fixtures are parsed and diffed at least four times each.
- **Six fixture-loader entry points**, two near-identical (`helper.rs:474` and `:521`, which
  also leaks a kept tempdir and prints on every call). ~60 hand-rolled "from_string ×2 +
  diff_code" blocks across 15 files; the public `diff_strings` has zero callers and one trivial
  test.
- **148 clamped mapping limits carry no `measured` line** (the painting clamps do; their slack is
  ≤0.03pp), so mapping drift toward the limit is invisible. Extend the census that exists for
  painting.
- **Zero tests**: `bin/analyze_human_mappings.rs` (1001 lines), the six `benchmark_other/*`
  output parsers (pure functions over other tools' stdout, the cheapest coverage win in `bin/`),
  `stats/git.rs`, `tui.rs`, `tui/events.rs`, `tui/actions.rs`, and `main.rs`'s file error paths
  (missing path, directory, invalid UTF-8; 30 tests exist, all argument parsing).
- **No Python tests at all**; `paper_variables.py:84` names a test that does not exist. Pure
  targets are listed in the survey: `latex_number`, `read_newcommands`, `bucket_index`,
  `ms_values`, `speed_percentiles`, `numstat_rows`, `ci_local.expand`, `coverage_report.area_of`.
- `viewer.js`: 496 of 574 lines sit below the DOM guard and are untested, including the
  "file an issue" URL builder. Both JS test files are bare `assert` with no case names.
- Duplicate test names across binaries (`help_modal_renders_keybindings`, `end_to_end`, ...)
  make `nextest -E` ambiguous. `human_solver`'s 216 tests all start from an empty mapping, which
  scores as perfect; audit which assertions depend on mapping content.

## 8. Order of attack

1. **Section 1 and 2 in one commit each**: the six defects, `build: test`, dependency gating,
   `[[bench]]` gating, CI/lint unification. All trivial, none touch the algorithm.
2. **Test wall-clock**: the six slow tests, the double painting diff, the shared `NodeCache`.
   Target: no test over 5s, suite under 60s CPU. Pure test-harness work.
3. **Dead code**: `DiffMode`/`--exact`/`fallback_used`, the `Suspend` chain, the two unused
   operations, the small orphans, the `#[cfg(test)]` gates, then the glob re-exports so the
   compiler takes over.
4. **Stale comments** (section 5, first bullet) and the history-to-TODO move. Zero risk, large
   clarity gain, best done as its own commit so diffs stay reviewable.
5. **Duplication, rendering side first** (items 1, 3, 6 of section 4), then `PassCtx`, then the
   algorithm-side pairs (4, 5) each verified with `make check-quality`.
6. **Research Python**: `_common.py`, pytest, the dead report script.
7. **Performance**: profile with the fixed bench, then the arena refactor (6.1-6.4), the Myers
   snapshot, the Fx sweep.

---

# Code Health Review — 2026-07-06

# HUMAN PLAYTESTING

* n/p should always say "There are changes in the other panel, would you like me to move you to the
  other panel" when you try to go beyond the last diff in the current panel and the other panel
  actually has changes. If the files are identical, a popup "Files are identical" is good.

Scope: duplicate code, code length, refactoring/generalization opportunities, structure,
readability. Ordered by expected payoff within each section. Line numbers are as of commit
f5e78e0.

## Status (2026-09-03)

The two module splits this document deferred at every previous review are done:
`bin/human_solver.rs` (15,297 lines) and `diff/apted/common.rs` (4,426) are now module
directories, split along the seams §2 named. `handle_modal_key` is 561 lines, down from 1,088.
§2 and §5 below are re-measured; the rest of this document still carries its 2026-07-07 numbers
and line references, which are stale.

## Status (2026-07-07)

Implemented in the working tree, verified by the full test suite (266 passed; only the two
pre-existing failures remain: `rust_algorithm_change` / `rust_turbopack_module_rule`
`matches_human_solution`) and by `benchmark_optimal_solutions` (178 mismatches / 13 unsolved —
byte-identical to a clean HEAD baseline run):

- **1.1 + 1.2 + 1.3** — `code::metadata::metadata_of` (Cow) replaced all 18 clone stanzas; the
  twin passes are now thin wrappers over `diff::hash_tree_matching::solve` (spec = hash accessor +
  classifier + reasons); the O(n) `mapping.iter().any` scans became O(1) `before_node_map`
  lookups; the identical pass got the structural pass's unclaimed-duplicate fix, and the pinning
  test now asserts one-to-one duplicate pairing.
- **1.4** — `NodeCache::build` folded into one `cache_for` helper; the `unsafe` transmute now has
  a single home.
- **1.5 + 1.6 + 1.9(blob)** — `stats::filesystem::find_git_repositories` (sorted; `commit_stats`
  is now deterministic), generic `stats::sampling::Reservoir<T>`, `stats::git::blob_bytes`.
- **§4 lints** — the four `spf_a` dead-store initializers removed; rustc's definite-assignment
  check proves the values were never read. Build is warning-free.
- **§4 typos** — `memo`, `unmatched`, forest/theoretically/mapping/maps/languages/its, plus
  `Therefore`/`something`/`fields` in code.rs/metadata.rs.

Not yet done (deliberately): the module splits of `human_solver.rs` / `apted/common.rs` (§2) and
the before/after side-parameterization (1.7/1.8) — the next candidates, per §5's order of attack.

---

## 1. Duplicate code

### 1.1 Metadata-clone boilerplate in every solver pass (also a real perf cost)

Every pass in the pipeline opens with the same 8-line stanza, once per side:

```rust
let before_metadata = before.metadata.ast_metadata.clone().unwrap_or_else(|| {
    crate::code::metadata::compute_ast_metadata(before).unwrap_or_default()
});
```

Sites (9 files, 18 clones): `solve_identical_trees.rs:37`, `solve_structurally_identical_trees.rs:42`,
`solve_semantically_structural_nodes.rs:115` **and** `:258` (twice in one file),
`solve_similar_flow_control.rs:56`, `solve_identical_diagnostic_statements.rs:62`,
`solve_moved_subtrees.rs:66`, `apted/common.rs:2293`.

This is not just repetition: `ASTMetadata` holds several whole-tree `HashMap`s
(`node_info`, `node_to_full_hash`, `full_hash_to_node`, structural-hash maps, …), so a single
`Diff::from_code` run deep-copies both sides' metadata ~7 times each. The comment at each site
("We clone to avoid lifetime issues") concedes the clone is incidental.

**Suggestion:** compute/ensure metadata once in `Diff::from_code` and pass `&ASTMetadata` for each
side into every pass (they already all share the `solve(before, after, &node_cache, &mut ast_diff)`
signature — extend it to take the two metadata refs). If a standalone entry point still needs the
fallback, one shared helper `fn metadata_of(code: &Code) -> Cow<'_, ASTMetadata>` replaces all 18
stanzas.

### 1.2 `solve_identical_trees` vs `solve_structurally_identical_trees` are near-clones with drift

The two passes share ~80% of their body line-for-line: same metadata stanza, same
reference-node loop, same "skip already mapped" check, same paired stack-descent matching children
by position and kind, same redundant `{ let after_node_id = matching_after_node.id(); … }` shadow
block. They differ only in (a) which hash map they consult (full vs structural), (b) how the
root/child operation is classified (always `Identical` vs text-compare → `Identical`/`Update`),
and (c) duplicate handling — and (c) is *accidental* drift, not a design difference:

- `solve_identical_trees.rs:91` takes `after_node_ids.iter().next()` — the documented
  duplicate-collapse bug (all N before-duplicates map onto one after-node; there's a 40-line
  comment plus a pinning test for it).
- `solve_structurally_identical_trees.rs:88` already fixed the same problem with
  `.find(|&&id| !diff.after_node_map.contains_key(&id))`.

**Suggestion:** extract one generic pass parameterized by (hash-map accessor, operation
classifier) — the structural pass's claimed-node filter then fixes the identical pass's TODO for
free. If the full extraction feels heavy, at minimum port the `.find(unclaimed)` fix and extract
the shared child-descent loop.

### 1.3 O(n) linear scans over `diff.mapping` inside per-node loops

Both twin passes ask "is this before-node already mapped?" via

```rust
diff.mapping.iter().any(|((before_id, _), _)| *before_id == before_node_id)
```

(`solve_identical_trees.rs:52` and `:133`; `solve_structurally_identical_trees.rs:57` and `:151`).
That is a full scan of the mapping table per reference node *and per descended child*, i.e.
O(nodes × mappings) per pass, when the O(1) answer already exists:
`diff.before_node_map.contains_key(&before_node_id)` (`add_mapping` keeps `before_node_map` in
lockstep with `mapping`, including delete/insert null entries, so the two checks are equivalent).
Fix falls out automatically if 1.2's extraction happens.

### 1.4 `NodeCache::build` — before/after bodies are copy-pasted verbatim

`diff.rs:63–117`: the two ~25-line closures building `before_cache` and `after_cache` are
byte-identical except for the variable they capture. Extract
`fn cache_for(code: &Code) -> HashMap<usize, Node<'static>>` (the `unsafe` transmute and its
SAFETY comment then live in exactly one place, which is also better for auditing the documented
soundness invariant).

### 1.5 `find_git_repositories` — three copies, one behaviorally drifted

- `sample_code_pairs.rs:122` and `sample_test_diffs.rs:129`: **byte-identical** (verified with
  `diff`), including the doc comment "Sorted for reproducible traversal order across runs".
- `commit_stats.rs:46`: older variant — extra `println!`s, and **no sorting**, so its traversal
  order is filesystem-dependent even though the two newer copies fixed exactly that.

**Suggestion:** the crate already has a library (`src/lib.rs`); move one canonical, sorted version
into a small shared module (e.g. `stats::filesystem` already exists as a home for fs helpers) and
delete the copies. `commit_stats` silently becomes reproducible too.

### 1.6 `Reservoir` — two copies, identical modulo element type

`sample_code_pairs.rs:100` (`items: Vec<Candidate>`) and `sample_test_diffs.rs:107`
(`items: Vec<Row>`) implement the same reservoir sampling `offer` line-for-line. A generic
`Reservoir<T>` in the same shared module as 1.5 replaces both (and both binaries' duplicated
`reservoir_never_exceeds_capacity` tests can collapse into one).

### 1.7 Systematic before/after mirror pairs in `apted/common.rs`

Pairs that differ only in which side's maps/costs/operations they touch:

| Before-side | After-side | Lines |
|---|---|---|
| `emit_before_subtree` | `emit_after_subtree` | 772 / 813 |
| `add_delete_mappings` | `add_insert_mappings` | 885 / 918 |
| `subtree_del_cost` | `subtree_ins_cost` | 951 / 970 |
| `filter_before_nodes` | `filter_after_nodes` | 1176 / 1184 |
| `before_match_target` | `after_match_target` | 1202 / 1215 |
| `collect_before_subtree_targets` | `collect_after_subtree_targets` | 1249 / 1283 |

That's ~250 lines of mirrored logic. Notably, `engine.rs::spf_a` already demonstrates the
side-parameterized style this file could use (`path_is_before: bool` selecting
`(idx, meta, del/ins)` at the top, one body). A small `struct SideCtx { meta, decision,
has_match_below, node_map, unit_cost_fn, null_slot }` — or an enum implementing the six accessors —
would halve this block and, more importantly, remove the standing risk of fixing a bug on one side
only (the same failure mode `MEMORY`/`HANDOVER` already track for `kinds_update_allowed`'s three
call sites).

Related micro-duplication: `UnitCostModel { language: … }` is constructed inline at ~6 sites in
the emit/cost helpers (`common.rs:739, 787, 828, 895, 928`, …). `ResolveCtx` could carry one.

### 1.8 Before/after mirror pairs in `human_solver.rs`

Same pattern at TUI scale — each pair differs only in which cache/map/glyph it reads:
`status_before`/`status_after` (742/763), `algo_status_before`/`algo_status_after` (837/845),
`algo_disagrees_before`/`algo_disagrees_after` (867/879),
`advance_before_to_next_unmarked`/`advance_after_to_next_unmarked` (949/956),
`clear_before_descendants`/`clear_after_descendants` (1188/1200). A `Side` enum **already exists**
(line 1883) but is not used to unify any of these. The file already passes `status_fn` as a
function pointer in places (`fully_solved_nodes`, `advance_to_next_unmarked`) — extending `Side`
with `fn caches_match(&self, &Caches)`, `fn removed(&self, &Caches)`, `fn mark_kind(&self)`
accessors would fold each pair into one function.

### 1.9 Smaller twins

- `test/helper.rs`: `was_tree_added` / `was_tree_deleted` (235/253) and
  `was_node_added` / `was_node_deleted` (225/230) differ only in the `(0, id)` vs `(id, 0)` key —
  one helper taking a key-constructor closure covers all four.
- `diff/text.rs::ranges` (42–248): the `DeleteWithChildren`, `InsertWithChildren`, `Delete`,
  `Insert`, and `Update` match arms are five copies of the same
  "`right_limit()` + build `RangeMatch` with operation X" block (differing in operation and the
  leaf-only guard). A 10-line helper `fn emit_at_right_limit(op, node, …)` shrinks the 200-line
  function by nearly half and makes the two genuinely distinct arms (`Identical`'s move
  detection, default descend) stand out.
- `bin/benchmark_diff_pairs.rs::blob_content` vs `bin/materialize_test_diffs.rs::blob_text` —
  same git-blob-lookup shape; candidate for the shared bin-support module of 1.5/1.6.

---

## 2. Code length

**Re-measured 2026-09-03.** The previous numbers were three reviews stale - this section listed
`human_solver.rs` at 4,298 lines when it was 15,297, and `apted/common.rs` at 3,901 when it was
4,426. Both splits it recommended have now been done.

Function-length outliers (non-test, by brace matching):

| Lines | Function | Verdict |
|---|---|---|
| 847 | `diff/text.rs:372 ranges` | **Worst remaining.** Was 207 here and predicted to "shrink naturally via finding 1.9"; it quadrupled instead. Next candidate. |
| 592 | `apted/engine.rs:581 spf_a` | Leave. Hand-tuned APTED port; profile before touching. |
| 561 | `bin/human_solver/events.rs handle_modal_key` | Was 1,088. Its three longest arms (`TextView` 270, `OpenDiffPicker` 182, `SolutionPicker` 105) are now functions taking only the values they use. The remaining 12 arms are 18-89 lines each and read fine as arms. |
| 492 | `bin/human_solver/events.rs handle_key` | **Leave.** 33 arms, longest 51 lines, 19 of them ten lines or fewer - a flat one-per-key dispatch table. It is long because the tool has many keys, not because any arm is bloated, and splitting a key table across functions makes it harder to read, not easier. |
| 485 | `diff/nodes.rs:502 is_semantically_structural` | Unexamined; a long `match` over node kinds, so possibly the same "legitimately wide table" case as `handle_key`. |
| 368 | `bin/analyze_human_mappings.rs:634 main` | Unexamined. |
| 328 | `bin/generate_mapping_site.rs:222 render_fixture_page` | Unexamined. |
| 266 | `bin/human_solver/events.rs handle_text_view` | Newly extracted from `handle_modal_key`; its own `:` line-prompt sub-mode is the obvious next seam if it grows. |

File-length outliers:

- **`bin/human_solver/` was one 15,297-line file**; it is now `main.rs` (1,850) plus `tests.rs`
  (6,239), `events.rs` (2,614), `render.rs` (1,498), `state.rs` (1,240), `actions.rs` (1,198),
  `navigate.rs` (392), `stubs.rs` (270), `flatten.rs` (259) - split along the section banners the
  file already carried.
- **`apted/common.rs` was 4,426 lines**; it is now 1,153 plus `common/{slots,residual,myers,
  resolve,prematch}.rs`, along the four concerns this document named.
- **`bin/human_solver/tests.rs` (6,239)** is now the largest file in the repo. Splitting it so each
  test sits beside the code it covers is worth doing and is *not* mechanical: the suite shares a
  large fixture set, so a test's body says more about which fixtures it borrows than about what it
  exercises, and two attempts at automatic routing both produced obviously wrong distributions.
  Lift the shared fixtures into their own `#[cfg(test)]` module first, then route by hand.
- `test/fixtures/**` long functions (e.g. `rust_turbopack_module_rule.rs`, 403 lines) are
  ground-truth data tables, not logic - exempt.

---

## 3. Structure & generalization

- **Pipeline pass signature is implicit convention, not a type.** All seven passes implement
  `fn solve(&Code, &Code, &NodeCache, &mut ASTDiff)` and `Diff::from_code` calls them in a
  carefully ordered sequence whose constraints live only in comments (Pass 3 after
  MatchSimilarFlowControl, diagnostics pass between the coarse passes and Pass 3 — both are
  memory-documented "easy to silently re-break" hazards). A minimal
  `trait DiffPass { const NAME: &str; fn solve(…); }` plus an ordered
  `const PIPELINE: &[…]` would (a) let the ordering constraints be asserted in one place or at
  least documented next to a single list, (b) give benchmarking/tracing a hook per pass, and
  (c) force new passes (1.1's metadata refs) through one signature change instead of seven.
- **`optimal_iud.rs` was deleted 2026-09-03.** This finding described it as "the exponential
  oracle used by `benchmark_optimal_solutions` and tests" - neither was true by then. Its only
  caller was `benches/optimal_iud_benchmark.rs`, which itself had no Makefile target and did not
  compile; its 16 tests tested only itself. 1777 lines, recoverable from git if it is ever wanted
  back as the starting point for a real oracle.
- **`NodeCache`'s transmuted `'static` lifetime** (diff.rs:40–54) is thoroughly documented, and
  callers are currently disciplined. If it ever grows another caller, consider the standard
  self-referential escape: make `NodeCache<'tree>` borrow properly and let the few construction
  sites own `Code` first. Documented-unsound-by-convention is the weakest structural point in an
  otherwise safe crate; no action urgent.
- **`ensure_parsed` / metadata-population responsibilities are split across `Code::parse`,
  `Code::ensure_parsed`, `from_string`, `from_file`** with slightly different guarantees (tests
  exist for each combination, which is itself a hint the state machine has too many entry
  states). Not urgent; worth a doc comment on `Code` stating which fields are guaranteed after
  which constructor.

---

## 4. Readability

- **Dead stores flagged by rustc in `engine.rs`** (`unused_assignments`): initializers at
  lines 577 (`current_forest_size2`), 579 (`current_forest_cost2`), 593 (`l_f_last`), 698 (`sp3`)
  are never read before being overwritten. In hand-ported numerical code these are harmless but
  they generate warning noise on every build; deleting the initial values (or restructuring to
  `let … = if …` where trivial) keeps the port shape while silencing the lint. Safe because the
  fuzz/oracle tests in `common.rs` pin behavior.
- **Typos in identifiers and docs** (all trivially fixable, some hurt search):
  "forrest", "theorethically",
  "maping" (diff.rs doc comments), "mapps" (solve_structurally_identical_trees.rs:31), test name
  `hello_world_translations_in_all_langauges` (diff.rs:787), recurring "it's" where "its" is meant
  (ASTMappingOperation docs).
- **Redundant shadow blocks** in both twin passes:
  `{ let after_node_id = matching_after_node.id(); … }` re-derives a value already bound two lines
  up and adds an indentation level to an already 5-deep nest (solve_identical_trees.rs:98,
  solve_structurally_identical_trees.rs:98). Goes away with 1.2.
- **Doc-comment style is split** between `/** … */` block style (older core files: diff.rs,
  helper.rs) and `///` line style (newer files: human_solver.rs, the newer
  passes). Rustfmt/idiom favors `///`; a mechanical conversion pass would make the codebase read
  uniformly, but do it in a dedicated commit so it doesn't pollute diffs.
- `Diff::from_code`'s pipeline comments are good; the `// TODO: switch to Algorithm::Apted once
  its performance gap vs Zhang-Shasha is resolved` (diff.rs:225) duplicates TODO.md state and can
  drift — pointing at TODO.md instead keeps one source of truth.

---

## 5. Suggested order of attack

**Items 1-4 of the original order are done** (see the Status section above and, for the two module
splits, the 2026-09-03 commits). What is left, in order:

1. **`diff/text.rs:372 ranges`, 847 lines** - the largest function in the codebase that is not a
   deliberately-wide table. This document predicted it would shrink on its own and it grew 4x
   instead, which is the strongest evidence here that a "shrinks naturally" verdict needs a
   re-measurement rather than trust.
2. **1.7 / 1.8 side-parameterization** - the before/after mirror pairs. Highest-value
   generalization left, and the module splits have made the affected code easier to see.
3. **Split `bin/human_solver/tests.rs`** so tests sit beside their code - blocked on lifting the
   shared fixtures out first, as §2 describes.
4. **`diff/nodes.rs is_semantically_structural`, 485 lines** - measure before splitting; it may be
   the same legitimate wide-table case as `handle_key`.

Not recommended: restructuring `engine.rs`'s APTED internals (`spf_a`, strategy functions) beyond
the lint-silencing in §4 — prior benchmarking discipline applies (profile via
`benches/diff_code_benchmark` first), and the mirrored `_l`/`_r` variants follow the published
algorithm's own structure.
