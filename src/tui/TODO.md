# TUI Usability Backlog

Originates from a 2026-08-07 usability/discoverability/information audit of the interactive TUI
(`src/tui/`), grounded in the actual code (`app.rs`, `diff_viewer.rs`, `ui.rs`, `help_modal.rs`)
and the README's "Using the TUI" section, not general advice. Organized into units meant to be
picked up and solved one at a time, roughly in priority order. Mark an item done in place
(`**Status:** IMPLEMENTED (date)`) rather than deleting it, same convention as `src/diff/TODO.md`.

## Bugs found during manual TUI testing, 2026-08-10 (DONE - all 3 fixed same day)

Three separate bugs reported after actually using the interactive TUI (`cargo run --bin codediff
-- BEFORE AFTER`), not a code-reading audit like the phases below. Investigated and fixed one at a
time; kept here in the same "problem/root cause/fix" shape as the phases below for the historical
record.

### Filename shown twice in dual-panel mode; no border at all in single-panel mode
- **Problem:** `DiffViewer::draw` (dual mode) drew an outer bordered `Block` per side showing
  " Before <filename>", *and* `CodeViewerWidget::render` drew its own bordered `Block` inside that
  showing "<filename> - <language>" - the filename appeared in both. Single-panel mode set
  `hide_border` on the inner widget (so only the outer border showed there), an inconsistency
  between the two modes for no real reason - dual mode's inner border existed only because it was
  "the only place the language shows at all", per the code's own comment.
- **Fix:** `CodeViewerWidget` no longer draws any border or title of its own at all (`hide_border`/
  `set_hide_border`/`inner_area` all removed - the widget always renders content flush with
  whatever area it's given). `DiffViewer::draw` now draws exactly one plain, borderless title row
  above the content in *both* modes (`panel_title` for dual, an inline `Paragraph` for single) -
  filename and language both fit on that one line, so nothing was lost by dropping the widget's
  own copy.
- **Files:** `widgets/code_viewer.rs` (removed the border/title branch from `render`, the
  `hide_border` field, `set_hide_border`, `inner_area`), `components/code_viewer.rs` (removed the
  `set_hide_border` pass-through; `cursor_screen_position`/`init` no longer inset for a border that
  doesn't exist), `components/diff_viewer.rs` (`draw`, new `panel_title` replacing `panel_block`).
- **Status:** IMPLEMENTED (2026-08-10). New tests: `render_never_draws_its_own_border_or_title`
  (`widgets/code_viewer.rs`), `draw_dual_panel_shows_each_filename_exactly_once` and
  `draw_never_draws_a_border_around_either_panel_in_either_mode` (`components/diff_viewer.rs`).

### Syntax highlighting silently missing for several common languages
- **Problem:** Not what Phase 4 below assumed (a missing *keybinding* for an otherwise-working
  toggle) - `syntax_highlighting` already defaults to `true` and nothing disables it. The real bug:
  `language_to_syntect` named several languages using syntect's own default (Sublime-stock) syntax
  names, and `SyntaxSet::load_defaults_newlines()`'s bundled set has no definition at all for Dart,
  Kotlin, Swift, TypeScript, or TSX, and uses different names than guessed for `ShellScript`
  ("Bourne Again Shell (bash)", not "Bash") and `ProtoBuf` ("Protocol Buffer" singular, not
  plural) - `find_syntax_by_name` silently returned `None` for all of these, so files in any of
  those languages rendered fully unstyled, with no error or indication anything was wrong.
- **Fix:** Switched `syntax_set()` from plain `SyntaxSet::load_defaults_newlines()` to
  `two_face::syntax::extra_newlines()` (new `two-face` dependency - bundles the much larger syntax
  set the `bat` CLI ships with) and corrected the three misnamed `language_to_syntect` entries.
  Bazel/Starlark remains a genuine, documented gap - present in neither syntax set.
- **Files:** `Cargo.toml` (new optional `two-face` dep, folded into the `tui` feature same as
  `syntect`), `widgets/code_viewer.rs` (`syntax_set`, `language_to_syntect`).
- **Status:** IMPLEMENTED (2026-08-10). New test:
  `every_language_except_the_documented_bazel_gap_resolves_to_a_real_syntax`.

### Real-terminal rendering corruption ("artifacts", repeated text, `?` doesn't clear it) on tab-indented files
- **Problem:** Reported via `cargo run -r --bin codediff -- .../before.html.test .../after.html.test`
  (a Hugo template file indented with real tab characters) - the TUI progressively corrupted its
  own display while scrolling, and opening the `?` help modal didn't clear the corruption
  underneath it. Root cause: `ratatui::buffer::Buffer` treats every character - `\t` included - as
  occupying exactly one cell, but a *real* terminal receiving a raw `\t` byte jumps its actual
  cursor to the next hardware tab stop instead. That desyncs the terminal's real cursor column
  from the column `ratatui`'s own Buffer model believes it's at; everything drawn afterward on that
  frame (and, since `ratatui` only ever redraws cells it believes changed, on later frames too)
  lands at the wrong screen position. This is invisible to a `ratatui::backend::TestBackend` test,
  which only models the Buffer, never an actual terminal's cursor-interpretation behavior - it had
  to be tracked down against the real bug report, not discovered by reading the code or existing
  tests.
- **Fix:** New `display_safe` helper replaces every `\t` with a single space when
  `DiffSessionData::before_contents`/`after_contents` are built (`assemble_diff_session_data`/
  `assemble_plain_text_diff_session_data`) - after the diff engine has already computed
  `before_ranges`/`after_ranges` against the original text, so this can't shift any of those
  offsets (`\t` and `' '` are both exactly one UTF-8 byte - a strict length-preserving swap, not a
  tab-stop-width expansion).
- **Files:** `app.rs` (`display_safe`, its two call sites in `assemble_diff_session_data`/
  `assemble_plain_text_diff_session_data`).
- **Status:** IMPLEMENTED (2026-08-10). New test:
  `compute_diff_never_puts_a_raw_tab_into_diff_session_data_contents`, against the real fixture
  that exposed the bug (`html-gohugoio-hugo-enclose-table-with-div-and-add-thead-tbody`), not a
  synthetic string.

## Phase 1: Persistent status/footer line (DONE, 2026-08-07 - all 5 sub-items implemented)

The single highest-value item: one component fixes several gaps below at once, and it's low-risk
since it doesn't touch existing keybinding dispatch. Today the *only* status feedback is a one-line
bar that appears solely for special categorical cases (new file, deleted file, whitespace-only,
comment-only, pure reformat/move, no changes) - for the most common case, an ordinary mixed edit,
nothing is shown at all (`app.rs`'s `status_bar_paragraph`/`DiffSummary`).

### Always-visible key hint bar
- **Problem:** The only way to learn any keybinding is pressing `?`. A first-time user has no
  signal that `?` even exists unless they've read the README first.
- **Solution:** A persistent footer line (like `less`/`vim`'s bottom bar) showing the handful of
  most-used keys: `?:help  o:open  n/p:next/prev  Tab:switch  q:quit`.
- **Files:** `app.rs` (`draw_viewer`), new small helper alongside `status_bar_paragraph`.
- **Impact:** Fixes the core discoverability gap without requiring a user to already know `?`
  exists.
- **Complexity:** Low.
- **Status:** IMPLEMENTED (2026-08-07). `App::draw_footer`, always-reserved row in `draw_viewer`;
  `FOOTER_HINTS` const.

### Change count summary for the common case
- **Problem:** `DiffSummary`'s categorical labels (`NewFile`, `WhitespaceOnly`, ...) cover the
  special cases; an ordinary mixed edit - the most common case - shows nothing.
- **Solution:** When `DiffSummary` doesn't apply (or alongside it), show actual counts, e.g.
  `+12 -4 ~2`, derived the same way `tui::headless`/`json_output` already compute their summaries.
- **Files:** `app.rs`, `diff::text` (existing line-operation counting).
- **Impact:** Every diff gets a real, glanceable summary, not just the special cases.
- **Complexity:** Low-Medium.
- **Status:** IMPLEMENTED (2026-08-07). `diff::text::change_counts` (counts each side's own
  `line_operations` independently, `Update` counted once from the after side only to avoid double
  counting); `App::change_counts` field, set on `Action::DiffReady`; `format_change_counts`.

### "Change N of M" during `n`/`p` navigation
- **Problem:** `n`/`p` jump straight to the next/previous change but give no sense of progress or
  how many changes remain - `next_change_position` (`widgets/code_viewer.rs`) just returns the next
  position, nothing about count or index.
- **Solution:** Track total change-range count and current index when jumping; show in the footer
  line, e.g. `change 3/12`.
- **Files:** `widgets/code_viewer.rs` (`next_change_position`), `components/diff_viewer.rs`
  (`jump_to_change`), `app.rs` (footer draw).
- **Impact:** Turns blind stepping into a real navigation aid, especially on large diffs.
- **Complexity:** Medium (needs a stored/derived total, not just a next-position lookup).
- **Status:** IMPLEMENTED (2026-08-07). `CodeViewerState::change_positions` factored out of
  `next_change_position` and reused by new `change_count_and_index` (counts change positions at or
  before the cursor, 1-indexed); threaded up through `CodeViewer`/`DiffViewer`.

### Cursor position indicator
- **Problem:** No line/column shown anywhere, unlike essentially every text editor.
- **Solution:** `Ln X, Col Y` in the footer line, sourced from the active `CodeViewer`'s existing
  cursor state.
- **Files:** `app.rs` (footer draw), `components/code_viewer.rs`/`widgets/code_viewer.rs` (cursor
  state already exists, just needs exposing).
- **Impact:** Standard editor affordance; near-zero cost since the state already exists.
- **Complexity:** Low.
- **Status:** IMPLEMENTED (2026-08-07). `DiffViewer::focused_cursor_position`.

### Active diff-mode indicator
- **Problem:** After the initial Fast/Exact prompt (`diff_mode_dialog.rs`) resolves, there's no
  ongoing indication of which mode produced the visible diff - a user can forget whether they're
  looking at the precise or approximate result.
- **Solution:** Small persistent label in the footer or panel title, e.g. `[fast]`/`[exact]`.
- **Status:** IMPLEMENTED (2026-08-07). `DiffSessionData::mode` (threaded through
  `assemble_diff_session_data`/`compute_diff`/`compute_diff_interactive`); `App::diff_mode` field;
  `format_diff_mode`.
- **Files:** `app.rs` (wherever the resolved `DiffMode` is already stored post-dialog).
- **Impact:** Removes a real source of confusion about result trustworthiness on big/unrelated
  files.
- **Complexity:** Low.

## Phase 2: Search / find-in-file (DONE, 2026-08-07)

- **Problem:** No way to search for text within a file; the only navigation aids are cursor
  movement and jump-to-next-change.
- **Solution:** A `/`-style search modal (same shape as `theme_dialog.rs`/`diff_mode_dialog.rs`),
  highlighting matches and jumping between them.
- **Files:** New `components/search_modal.rs`, wiring in `app.rs`/`actions.rs`.
- **Impact:** Standard, expected editor/pager feature; currently the biggest missing usability
  primitive.
- **Complexity:** Medium.
- **Status:** IMPLEMENTED (2026-08-07). `/` opens `SearchModal` (new text-entry dialog, same visual
  scaffold as `ThemeDialog`/`FileDialog` but no list to select from). Enter emits
  `Action::SearchSubmitted`, resolved by `App::handle_search_submitted` into
  `DiffViewer::search`/`CodeViewer::search`/`CodeViewerWidget::find_matches` (case-insensitive,
  per-line substring search - matches never span a line break). `>`/`<` step between matches
  (`CodeViewerState::next_search_match_position`), a distinct pair from `n`/`p` rather than
  overloading them, since `help_modal.rs` already documents those as change-navigation. Matches
  render in the same `cross_highlight_bg` blue as the cursor's cross-panel highlight - a dedicated
  search color was deliberately deferred (would need touching `OverlayPalette` across all 8 themes
  and their distinctness tests, a separate chunk of work with its own failure mode). The footer's
  `match N/M` replaces `change N/M` while a search is active rather than showing both, to stay
  inside the footer's fixed-width left column. `CodeViewerState::next_position`/`count_and_index`
  were extracted as free functions shared between change-navigation (`n`/`p`) and search-navigation
  (`>`/`<`), which turned out to need identical wrap-around/counting logic.

## Phase 3: Jump-to-line

- **Problem:** No way to jump directly to a known line number - only linear scrolling or
  change-hopping.
- **Solution:** A `g` (or `:`-style) prompt that takes a line number and scrolls/moves the cursor
  there, mirroring `vim`'s `gg`/`:N` convention already implied by this TUI's `h/j/k/l` scheme.
- **Files:** New small input-prompt component (could share shape with the search modal above),
  `app.rs`/`actions.rs`.
- **Impact:** Useful when cross-referencing with an external line number (stack trace, review
  comment, etc.).
- **Complexity:** Low-Medium.

- **Status:** IMPLEMENTED (2026-08-19). `g` opens `components/line_prompt.rs` (digits-only
  input, Enter jumps 1-indexed and centers, Esc/empty cancels); `Action::JumpToLineSubmitted`,
  `DiffViewer::jump_to_line`.

## Phase 4: Syntax-highlighting toggle keybinding

- **Problem:** The widget layer (`widgets/code_viewer.rs`) already supports toggling syntax
  highlighting, but nothing in the running app calls it - confirmed during the 2026-08-07 code
  health pass, which found and removed the *component*-layer pass-through methods
  (`enable_syntax_highlighting`/`disable_syntax_highlighting`/`is_syntax_highlighting_enabled`) as
  dead code, since no keybinding ever reached them.
- **Solution:** Add a keybinding (e.g. `S`) that re-adds a thin component-layer call into the
  still-live widget-layer toggle, and document it in `help_modal.rs`/README.
- **Files:** `components/code_viewer.rs` (re-add the wrapper removed in the health pass, this time
  wired to a key), `app.rs` (key dispatch), `help_modal.rs`.
- **Impact:** Restores a real, previously-dead-ended feature as an actual user-facing toggle.
- **Complexity:** Low.
- **Note (2026-08-10):** Highlighting itself was never actually off (`syntax_highlighting` already
  defaults to `true`) - the missing-toggle-keybinding gap described above is real and still open,
  but it's not why a user would see *no* highlighting at all. That turned out to be a different,
  now-fixed bug: see "Syntax highlighting silently missing for several common languages" above.

- **Status:** IMPLEMENTED (2026-08-19). `S` toggles both panels at once
  (`DiffViewer::toggle_syntax_highlighting`, component-layer wrappers re-added in
  `components/code_viewer.rs`); documented in `help_modal.rs`.

## Phase 5: Change overview / minimap

- **Problem:** On a large file, there's no way to see *where* all the changes are at a glance -
  only linear `n`/`p` stepping through them one at a time.
- **Solution:** A narrow sidebar or scrollbar-adjacent strip marking change locations by color
  (insert/delete/update/move), similar to what many GUI editors show next to the minimap/scrollbar.
- **Files:** New widget, `components/diff_viewer.rs` (layout), `theme.rs` (reuse existing diff
  colors).
- **Impact:** Meaningfully faster orientation on large diffs; the most visually involved item here.
- **Complexity:** Medium-High.

- **Status:** IMPLEMENTED (2026-08-19). One-column strip at each panel's right edge
  (`carve_minimap`/`render_minimap` in `diff_viewer.rs`, bands from
  `CodeViewer::change_bands`), painted in the active theme's own operation colors;
  delete > insert > update > move priority when a band holds several kinds.

## Phase 6: Mouse scroll support

- **Problem:** `ui.rs` already maps `crossterm` mouse events into this TUI's own `Event::Mouse`,
  but mouse capture is disabled by default (`UI::mouse` defaults to `false`) and `app.rs`'s
  top-level dispatch explicitly no-ops every mouse event (`Event::Key(_) | Event::Mouse(_) => {}`).
- **Solution:** Enable mouse capture by default and wire at least scroll-wheel up/down to the
  same scroll behavior as `Page Up`/`Page Down` (or `j`/`k`) on the panel under the cursor.
- **Files:** `ui.rs` (`UI::new`'s `mouse: false` default), `app.rs` (event dispatch).
- **Impact:** Matches baseline expectations for a modern terminal UI; currently scroll wheel does
  nothing at all.
- **Complexity:** Low-Medium (mostly about picking which panel a mouse event over a given
  column/row belongs to).

- **Status:** IMPLEMENTED (2026-08-19), including the extensions from the 2026-08-19 pass:
  capture on by default (`ui.mouse = true` in `App::run`), wheel scrolls the panel under the
  pointer 3 lines a notch, left click focuses that panel and places the cursor on the clicked
  character (`DiffViewer::handle_mouse_event`, hit-testing via content rects recorded in
  `draw`).

## 2026-08-19 usability/QoL pass

A second audit, grounded in the current code (`app.rs`, `diff_viewer.rs`, `widgets/code_viewer.rs`,
`headless.rs`, the dialogs) plus smoke tests against the built binary. Phases 3 (jump-to-line),
4 (syntax-highlight toggle key), 5 (minimap), and 6 (mouse scroll) above are still open and still
worth doing; nothing below duplicates them. Ordered by priority.

### Pure sibling-reorder diffs report literally nothing (likely a bug, investigate first)
- **Problem:** Two files that differ only by reordering two top-level functions render in headless
  mode as `... 9 unchanged lines ...` on both sides - no moved-chunk box, no `~` markers, and no
  `DiffSummary` line either. The bytes differ, yet the output is indistinguishable from a
  whitespace-only formatting pass being absent entirely. `DiffSummary::RefactorMovedOnly` exists
  precisely for this shape but does not fire - plausibly because the commutative-container work
  made reorders cost-neutral *matches* rather than `Move` operations, and both the summary
  classifier and the box renderer key off `TextOperation::Move`. Reproduce: two single-function
  swaps, `codediff b2.rs a2.rs` (verified 2026-08-19).
- **Solution:** First decide the intended semantics: if a commutative reorder is deliberately "no
  semantic change," the summary should still say so explicitly ("Declarations reordered only"),
  not stay silent; if reorders should render as moves, the range assembly needs to emit `Move` for
  crossing matches again. Either way, silence is the one wrong answer.
- **Files:** `diff/text.rs` (`summarize_diff_with_comment_check` / operation assignment),
  `headless.rs` only if the box path is involved.
- **Impact:** Trust: a diff tool whose output says "unchanged" for changed files undermines every
  other number it prints.
- **Complexity:** Low once the intended semantics are decided; the investigation is the work.

- **Status:** IMPLEMENTED (2026-08-19). Root cause confirmed: `ranges`'s `Identical` arm only
  produced `Move` on a *column* shift, so row-only relocations rendered as unchanged. Fix: the
  `crossed_backwards` check - a multi-row matched node whose destination starts before the
  last sequential anchor is a `Move` (sub-line tokens are exempt, so imperfect leaf matches
  can't paint noise). A pure reorder now renders the moved-chunk box on both sides and
  `RefactorMovedOnly` fires. Known remaining quiet case, deliberate: a reorder whose blocks
  also contain edits decomposes into single-row/sub-line matches that the multi-row guard
  exempts - the edits themselves still render, only the relocation stays unmarked.

### Line numbers (TUI gutter + headless output)
- **Problem:** No line numbers anywhere. The TUI panels have no gutter (`widgets/code_viewer.rs`
  renders content flush; only the footer's `Ln X, Col Y` names the cursor line). Worse, headless
  output *references* line numbers it never prints: the moved-chunk header says `Moved to lines
  40-60` and the `@` landmark prefixes a hunk with the enclosing declaration, but a reader has no
  way to locate line 40 in an output stream with no numbers - the cross-reference is currently
  unresolvable by eye.
- **Solution:** A dimmed right-aligned line-number gutter in `CodeViewerWidget::render` (sliced
  with the viewport; width from total line count) and a matching per-line number prefix on kept
  lines in `headless.rs`'s `render_side`. Both on by default; elision markers then also become
  self-locating.
- **Files:** `widgets/code_viewer.rs` (`render`), `headless.rs` (`render_side`, tests),
  `json_output.rs` untouched (already carries row indices).
- **Impact:** Makes the moved-chunk headers actually followable and matches baseline pager/editor
  expectations; also the prerequisite for jump-to-line (Phase 3) to feel useful.
- **Complexity:** Medium (marker column layout in headless shifts every existing rendering test).

- **Status:** IMPLEMENTED (2026-08-19). Dimmed right-aligned gutter in
  `CodeViewerWidget::render` (width from line count, absent on an empty panel); headless
  prefixes every kept line and the `@` breadcrumb with its 1-indexed number.

### Reload / re-diff key (`r`)
- **Problem:** The natural workflow - see the diff, fix the code in an editor, check again -
  requires quitting and relaunching, or re-picking both files through `o`. `App::before_path`/
  `after_path` already remember everything needed.
- **Solution:** `r` in `Viewer` re-reads both paths and re-runs `start_diff`, preserving the
  cursor/scroll position where the row still exists. Document in `help_modal.rs` + footer.
- **Files:** `app.rs` (key dispatch, `start_diff` re-entry), `help_modal.rs`.
- **Impact:** Turns the TUI from a one-shot viewer into something usable inside an edit loop;
  probably the highest value-per-line-of-code item here.
- **Complexity:** Low.

- **Status:** IMPLEMENTED (2026-08-19). `r` re-diffs the remembered pair; the cursor is
  restored (clamped) after `DiffReady` via `App::restore_after_reload`.

### Horizontal scrolling for long lines
- **Problem:** There is no `scroll_col` anywhere: the viewport never scrolls horizontally, long
  lines are hard-truncated at the panel width, and `l`/Right happily moves the cursor past the
  right edge - the cursor (and the range under it) just leaves the visible area with no indicator
  that content is cut off.
- **Solution:** Track a horizontal scroll offset in `CodeViewerState`, keep the cursor column in
  view the same way `sync_scroll` keeps the row in view, and render a truncation indicator (`…`)
  at a cut edge. Slicing the cached styled lines by column span is the fiddly part (styled spans,
  not raw chars).
- **Files:** `widgets/code_viewer.rs` (state + render), `components/diff_viewer.rs` (`sync_scroll`).
- **Impact:** Long-line files (generated code, minified JS/CSS, long string literals) are currently
  partially unviewable; this class of file is common in the corpus.
- **Complexity:** Medium.

- **Status:** IMPLEMENTED (2026-08-19). `CodeViewerState::scroll_col`/`viewport_width`,
  cursor-following via `scroll_to_show_col`, styled-span slicing with dimmed `…` truncation
  markers (`slice_columns`).

### Exit-code convention for non-interactive modes
- **Problem:** `codediff a b` exits 0 whether the files are identical or different (verified
  2026-08-19). Every scripted caller coming from `diff`/`git diff` expects 0 = same, 1 = differs,
  2 = trouble; today the only way to detect "no changes" is parsing the output text or JSON.
- **Solution:** Adopt the `diff` convention in headless and JSON modes (interactive TUI stays 0).
  Decide explicitly what a semantically-neutral-but-textually-different file pair (reorder,
  whitespace-only) returns - the `DiffSummary` classification is the natural source; consider `0`
  only for byte-identical, and document it in `--help`.
- **Files:** `main.rs`, `headless.rs`/`json_output.rs` (return the classification outward).
- **Impact:** Makes `codediff` usable in scripts/CI conditionals at all.
- **Complexity:** Low.

- **Status:** IMPLEMENTED (2026-08-19). 0 = byte-identical, 1 = differs, 2 = error for
  headless/JSON `BEFORE AFTER` invocations; the 7-argument `GIT_EXTERNAL_DIFF` form always
  exits 0 on success (git treats non-zero as fatal, verified against difftastic's documented
  handling of the same constraint). Documented in `--help`'s after-help.

### Cancel an in-flight diff without quitting
- **Problem:** During `AppScreen::Diffing`, `Esc` quits the whole app (`esc_should_quit`), and
  there is no way to abandon a slow diff and return to the viewer - on a pathological pair the
  only options are waiting or losing the session.
- **Solution:** `Esc` during `Diffing` returns to `Viewer` and marks the pending computation stale
  (generation counter checked when `DiffReady` arrives - the `spawn_blocking` task itself can't be
  killed, but its result can be discarded), keeping whatever diff was previously loaded.
- **Files:** `app.rs` (`start_diff`, `DiffReady` handling, `esc_should_quit`).
- **Impact:** Removes the worst-case interaction (losing the session to a slow diff).
- **Complexity:** Low-Medium.

- **Status:** IMPLEMENTED (2026-08-19). Esc during `Diffing` bumps `App::diff_generation` and
  returns to the viewer; the stale `DiffComputed` result is dropped on arrival. Esc no longer
  quits from that screen at all.

### Rethink the blocking Fast/Exact modal
- **Problem:** When `PendingDiff::looks_expensive()` trips, the user is stopped by a modal choice
  *before seeing anything*, and must decide fast-vs-exact blind. Post-pipeline-rework, fast mode's
  quality is far closer to exact and its latency is low; the modal's cost/benefit has shifted since
  it was designed.
- **Solution:** Always compute fast immediately and render it (footer already shows `[fast]`);
  offer a keybinding (e.g. `x`) that re-runs exact in the background and swaps the result in when
  ready. Removes the modal entirely; `--exact` stays for batch. Fold into the pending pipeline
  Phase 5 cleanup, which already owns the `DiffMode` surface question.
- **Files:** `app.rs` (`compute_diff_interactive`, `SelectDiffMode` screen removal),
  `components/diff_mode_dialog.rs` (deleted), `help_modal.rs`.
- **Impact:** One less interruption, and the choice becomes informed (you're looking at the fast
  result while deciding whether exact is worth it).
- **Complexity:** Medium.

- **Status:** IMPLEMENTED (2026-08-19), with a different second half than planned: the modal,
  `SelectDiffMode` screen, and `diff_mode_dialog.rs` are gone and every diff computes
  immediately - but the planned `x` re-run-exact key was *not* added, because investigation
  showed `PendingDiff::finish` has ignored `DiffMode` entirely since the phases-4-7
  rearchitecture (see its phase-6 comment): there is no exact path for `x` to select, and
  shipping a key that recomputes an identical result would be a placebo. The footer's
  `[fast]`/`[exact]` labels are removed for the same reason (`[plain text]` stays - that
  distinction is real), `--exact` is documented as a deprecated no-op, and headless mode's
  large-residual note no longer claims `--exact` would change the result. `DiffMode`'s
  removal belongs to the pipeline's own pending Phase 5 cleanup.

### Jump to counterpart (`Enter`)
- **Problem:** The cross-highlight shows where the range under the cursor went on the other panel,
  but there's no way to *go* there - for a long-distance move you can see the blue block scroll by
  on the other side but must manually Tab over and navigate to it.
- **Solution:** `Enter` on a range with a `destination` switches the active panel and places the
  cursor at the destination's start (the data is already computed for the cross-highlight).
  Pressing it again jumps back - free round-trip navigation for moves.
- **Files:** `components/diff_viewer.rs` (new action on the existing `sync_cross_highlight` data),
  `help_modal.rs`.
- **Impact:** Makes move-heavy diffs (the tool's differentiating feature) navigable, not just
  visible.
- **Complexity:** Low.

- **Status:** IMPLEMENTED (2026-08-19). `DiffViewer::jump_to_counterpart`; pressing Enter
  again jumps back.

### Search QoL bundle
- **Problem:** Search (`/`) works but is bare: no feedback until Enter (the modal shows no live
  match count), no way to repeat the last search after clearing it, matches share the same blue as
  the cursor cross-highlight (the deliberate deferral recorded in Phase 2 above - while a search is
  active, "search hit" and "counterpart of cursor" are visually identical), and matching is always
  case-insensitive with no smart-case.
- **Solution, in value order:** (1) live match count in the modal while typing (`12 matches`,
  reusing `find_matches` incrementally); (2) remember the last query - `/` `Enter` repeats it;
  (3) a dedicated search highlight color across the 8 `OverlayPalette`s + their distinctness
  tests; (4) smart-case (case-sensitive iff the query contains an uppercase char).
- **Files:** `components/search_modal.rs`, `widgets/code_viewer.rs`, `theme.rs` (item 3).
- **Impact:** Each small; together they make search feel finished rather than minimal.
- **Complexity:** Low each; item 3 is the widest (all themes + tests).

- **Status:** IMPLEMENTED (2026-08-19), all four: live match count in the modal title (via
  `Action::SearchQueryChanged` + highlight preview without cursor movement), bare-Enter
  repeats the last query (hint line advertises it), a dedicated `search_bg` orange across all
  8 palettes (+ distinctness test), and smart-case matching.

### Scroll-without-cursor keys
- **Problem:** Every navigation key moves the cursor; the only large motions are PageUp/Down and
  Home/End. Vim muscle memory expects `Ctrl-d`/`Ctrl-u` (half page, cursor follows) and
  `Ctrl-e`/`Ctrl-y` (scroll view one line, cursor stays) - none are bound.
- **Solution:** Bind all four in `diff_viewer.rs`'s key handler; half-page is PageUp/Down's logic
  with `height/2`, view-scroll needs a small "scroll offset without cursor sync" path.
- **Files:** `components/diff_viewer.rs`, `widgets/code_viewer.rs`, `help_modal.rs`.
- **Impact:** Comfort feature for the vim-keyed audience this TUI already targets with `hjkl`.
- **Complexity:** Low.

- **Status:** IMPLEMENTED (2026-08-19). Ctrl-d/Ctrl-u half-page cursor moves, Ctrl-e/Ctrl-y
  view-only scrolling (both panels in dual mode, matching PageUp/PageDown's convention).

### Dynamic color legend + theme-dialog live preview
- **Problem:** Two sides of one gap. `help_modal.rs` deliberately omits a color legend because the
  colors are theme-dependent - but that's exactly why a *static* legend was wrong, not why no
  legend should exist. And `theme_dialog.rs` applies a theme only on Enter: choosing between 8
  palettes means Enter/reopen/Enter/reopen.
- **Solution:** Render the legend dynamically from the live `OverlayPalette` (colored swatches:
  `■ inserted ■ deleted ■ moved ■ updated ■ cursor/counterpart`) in the help modal and as a strip
  inside the theme dialog; apply the highlighted theme immediately on selection change and revert
  on Esc.
- **Files:** `components/help_modal.rs` (needs the palette passed in - it's currently static),
  `components/theme_dialog.rs`, `app.rs`.
- **Impact:** New users stop guessing what magenta means; theme choice becomes one keystroke
  instead of a loop.
- **Complexity:** Low-Medium.

- **Status:** IMPLEMENTED (2026-08-19). Help modal renders swatches from the live palette
  (`HelpModal::legend_lines`); the theme dialog previews the highlighted theme on every
  selection move (`Action::ThemePreviewed`), Esc reverts, Enter persists.

### Move count in the footer summary
- **Problem:** `format_change_counts` shows `+12 -4 ~2`; moves are invisible in the summary even
  though move detection is the tool's headline feature. A move-heavy diff summarizes as almost
  nothing.
- **Solution:** Count moved ranges (source side once) in `diff::text::change_counts` and append
  e.g. `M3`, in the Move overlay color. Also surfaces the sibling-reorder silence above if the
  first item decides reorders count as moves.
- **Files:** `diff/text.rs` (`ChangeCounts`/`change_counts`), `app.rs` (`format_change_counts`).
- **Complexity:** Low.

- **Status:** IMPLEMENTED (2026-08-19). `ChangeCounts::moves` (counted once, after side),
  rendered as `M<n>`.

### Manual layout override for the 220-column threshold
- **Problem:** `SINGLE_PANEL_THRESHOLD = 220` means a 200-column terminal - wide by most standards
  - is forced into single-panel mode with no recourse, even though two 100-column panels are
  perfectly readable for most code.
- **Solution:** A key (e.g. `v`) cycling `auto → dual → single` (persisted next to the theme via
  the existing `confy` config), with `auto` keeping today's threshold. Possibly lower the
  threshold's default while at it - 160 gives two 80-column panels.
- **Files:** `components/diff_viewer.rs` (`display_mode`), `theme.rs`-style config, `help_modal.rs`.
- **Impact:** Respects the user's judgment about their own terminal.
- **Complexity:** Low.

- **Status:** IMPLEMENTED (2026-08-19). `v` cycles auto/dual/single
  (`theme::PanelLayout`, persisted in `.codediff.toml` alongside the theme via
  load-modify-save); footer shows `[layout: …]` when overriding. The 220 default is
  unchanged - the override makes it moot.

### Headless `--color` and `--context` flags
- **Problem:** Headless colors are on by default with `NO_COLOR` as the only escape - so
  `codediff a b > out.txt` writes ANSI garbage unless the caller knows the env var; and
  `CONTEXT_LINES = 3` is hardcoded with no equivalent of `diff -U N`.
- **Solution:** `--color <always|never>` (flag beats env var; keep `NO_COLOR` honored) and
  `--context N` threaded into `lines_to_keep`. Both standard CLI furniture.
- **Files:** `main.rs` (args), `headless.rs`.
- **Complexity:** Low.

- **Status:** IMPLEMENTED (2026-08-19). `--color <auto|always|never>` (flag beats `NO_COLOR`
  in both directions), `--context N` threaded into `lines_to_keep`.

### File dialog QoL
- **Problem:** The picker is scroll-only: no type-ahead filtering, no hidden-file toggle, no way
  to paste/type a path, and no memory of previously diffed pairs - the empty-start screen always
  begins from scratch in the cwd.
- **Solution, in value order:** (1) type-to-filter the current listing (chars narrow, Backspace
  widens, Esc clears filter before closing); (2) recent file pairs persisted via the existing
  `confy` config, offered on the empty-start screen ("press 1-9 to reopen a recent pair"); (3) a
  `.`-key hidden-files toggle.
- **Files:** `components/file_dialog.rs`, `app.rs`, config module.
- **Complexity:** Medium (mostly the recent-pairs plumbing).

- **Status:** IMPLEMENTED (2026-08-19). Type-to-filter (Backspace widens the filter before
  reverting to parent-directory navigation; `..` always survives the filter), `Ctrl-h`
  dotfile toggle (hidden by default), and recent pairs persisted in `.codediff.toml`
  (recorded on every successful diff, digit keys 1-9 reopen them from the empty-start
  screen's overlay).

### Open at cursor in `$EDITOR`
- **Problem:** The end of most diff-reading sessions is "now go fix it" - which means manually
  noting the filename and line, quitting, and opening an editor.
- **Solution:** `e` suspends the TUI (`crossterm` leave/enter alternate screen around a child
  process), launches `$EDITOR +<line> <path>` for the focused panel's file at the cursor row, and
  resumes (re-diffing on return, which the `r` reload item above provides for free).
- **Files:** `app.rs`, `ui.rs` (suspend/resume), `help_modal.rs`.
- **Impact:** Closes the loop on the tool's main workflow.
- **Complexity:** Medium (terminal state handoff is the risky part; get `r` in first).

- **Status:** IMPLEMENTED (2026-08-19). `e` releases the terminal (`ui.exit`/`ui.enter`, not
  `suspend` - that raises SIGTSTP), runs `$VISUAL`/`$EDITOR` (`vi` fallback) with the `+line`
  convention, then re-diffs with the cursor restored.

### Mouse (Phase 6) extensions, once wheel scroll lands
Click on a panel to focus it (replacing a Tab press), click on a row/column to place the cursor
there (the hit-testing needed for wheel-scroll's "which panel is the pointer over" gives both
nearly for free). Recorded here so Phase 6 isn't scoped to wheel-only when the marginal cost of
clicks is this low.
