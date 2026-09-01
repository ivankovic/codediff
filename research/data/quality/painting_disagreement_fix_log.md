# Fix log for painting_disagreement_census_2026_09_01.md

Working log, not a findings doc - updated live while attempting a fix for every row in the census
table, Minimal and Full separately, per instruction: nothing is too small, skip only rows that
require N:M mapping. Status per row: `todo` / `fixed` / `reverted` (attempted, regressed something
else, backed out - reason recorded) / `skip-nm` (needs N:M mapping) / `no-fix-found`.

**Process for every attempt:** change → `cargo test --lib --features test-fixtures
handmade_painting_disagreement_report -- --ignored --nocapture` (whole handmade corpus, both
modes) + `measure_stub_fixtures` for the 6 `small`-dataset fixtures + the fixture's own
`painting_agreement` test + full `cargo test --lib` for algorithm-level regressions (mapping
quality, not just painting) → keep only if nothing else got worse.

## Skipped as N:M

- kotlin-refactor-function (functions merge into a class, N old nodes ↔ M new nodes)
- javascript-add-destructuring (4 assignments → 1 destructuring statement)
- javascript-typescript-interesting-small-edit-refactor (destructuring collapse, same shape)
- rust-multi-map-duplicate-calls (3 identical calls ↔ 2, genuinely ambiguous N:M tie-break)
- javascript-add-event-listener (token reuse across differently-shaped nodes - re-evaluate: this
  may be 1:1 per token, not N:M - revisit before skipping for real)

## Fixed

- **python-api-change** (confirmed bug, both modes): `trim_trailing_whitespace` (`src/diff/text.rs`)
  used `lines.get(end_row)?` unconditionally. A range that runs through EOF in a file with no
  trailing newline gets `end_row == lines.len()` (one past the last real line - same convention as
  the trailing-newline case, which has a genuine phantom empty `lines` entry there; a file without
  one doesn't, so the lookup returned `None` and the whole range was silently dropped instead of
  trimmed). Root-caused via a `dump_top_level_mapping`/`DUMP_RAW_RANGES` diagnostic added to
  `human_mapping.rs`: confirmed the AST diff and the *raw* `ranges()` output were both already
  correct (a full Insert range for `get_user_info`, rows 22-33) and the drop happened specifically
  inside `ranges_for_options`. Fixed by treating `end_row >= lines.len()` the same as the existing
  "previous row's newline" branch: step back to the last real row's own end instead of bailing out.
  minimal 19.961%→6.610%, full 26.309%→4.319% (residual is ordinary pattern-1 tail-shift noise on
  the docstring/paren, not this bug). **Verified zero regressions**: `handmade_painting_disagreement_report`
  byte-for-byte identical on all other 54 measured fixtures, before and after.

- **Pattern 3, all affected fixtures** (connecting-newline seam): confirmed this was never a
  product bug - `code_viewer.rs`/`headless.rs` already bound every row's painted columns to
  `TextRange::columns_on_row(row, row_len)`, and `row_len` (from `str::split('\n')`-derived
  `lines`) never includes the newline, so the real renderer has *never* highlighted an interior
  row's connecting newline. Only the test harness's `label_bytes` (human_mapping.rs) filled every
  raw byte in a span including literal `\n` bytes, measuring a disagreement no reader of the real
  output could ever see. Fixed by forcing any `\n` byte's label to `None` after building
  `label_bytes`, applied uniformly to both the codediff side and the human side (so a human's own
  multi-row entry is held to the same floor). **Zero regressions, ~30 fixtures improved**,
  `rust-adding-many-identical-cfg-test-statements-...` and
  `rust-adding-to-a-list-of-identical-attributes-...` now measure exactly 0.000%/0.000%. Aggregate
  1.3048%→1.2590%.

- **Pattern 4, all 3 affected fixtures + ~37 others** (`cpp-optimize-algorithm`,
  `javascript-add-array-method`, `kotlin-add-validation`, plus large partial improvements
  everywhere a `MINIMAL` fixture has any multi-row insert/delete): flipped
  `RenderOptions::MINIMAL.interior_line_indentation` from `true` to `false` - Minimal now trims
  each interior row to its own content (rules doc's indentation choice 2), matching what the
  corpus's own Minimal ground truth wants almost everywhere. Measured empirically first (disabled
  the guarding `const` assertion, ran the full corpus, only then made it permanent) rather than
  guessed: handmade aggregate 1.2590%→1.1811%, ~40 fixtures improved (`python-refactoring`
  7.544%→0.677%, `python-api-change` 5.759%→1.113%, `cpp-optimize-algorithm` 5.751%→0.319%,
  `javascript-add-array-method` 8.312%→0.519%, `kotlin-add-validation` 4.553%→0.325%), **one
  regression**: `rust-add-to-existing-use` 10.490%→11.189% (Minimal only; its own disagreement is
  dominated by an unrelated single-row column-shift Move, pattern 1 - not investigated further,
  left as its own `todo` row below). Updated `interior_line_indentation`'s doc comment (previously
  claimed "no fixture paints choice 2 yet" - no longer true) and replaced the guarding test
  (`neither_preset_turns_off_interior_line_indentation` → `presets_disagree_on_interior_line_indentation`)
  to pin the new, opposite fact instead of the old one. Fixed 2 stale expected-value literals in
  `render_options_dialog.rs`'s own tests (mechanical fallout of the new default, not a separate
  bug). **7 previously-failing `painting_agreement`/pre-existing-failure tests now pass**:
  `cpp-optimize-algorithm`, `javascript-add-array-method`, `javascript-add-event-listener`,
  `java-add-exception-handling`, `kotlin-add-null-check`, `python-add-remove-block`,
  `rust-cost-optimization`.

## Pre-existing, unrelated to this task

`cargo test --lib --features test-fixtures` has 16 pre-existing failures (confirmed present on the
baseline commit before any fix attempted here, via `git stash`): stale `assert_matches_human_painting_within_limit`
clamps that are tighter than the currently-measured rate, for `cpp-optimize-algorithm`,
`java-add-exception-handling`, `javascript-add-array-method`, `javascript-add-event-listener`,
`javascript-fix-promises`, `kotlin-add-data-class`, `kotlin-add-null-check`, `kotlin-add-validation`,
`python-add-remove-block`, `python-bugfix-loop`, `python-refactoring`, `rust-add-if`,
`rust-cost-optimization`, plus 3 `optimal_solutions` tests. Not touching clamp values as "fixes" -
only the measured rate going down counts; a clamp gets tightened only as a side effect of a row
actually being fixed.

## Row status

(fixture, minimal, full - todo until attempted)
