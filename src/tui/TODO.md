# TUI Usability Backlog

Originates from a 2026-08-07 usability/discoverability/information audit of the interactive TUI
(`src/tui/`), grounded in the actual code (`app.rs`, `diff_viewer.rs`, `ui.rs`, `help_modal.rs`)
and the README's "Using the TUI" section, not general advice. Organized into units meant to be
picked up and solved one at a time, roughly in priority order. Mark an item done in place
(`**Status:** IMPLEMENTED (date)`) rather than deleting it, same convention as `src/diff/TODO.md`.

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

## Phase 5: Change overview / minimap

- **Problem:** On a large file, there's no way to see *where* all the changes are at a glance -
  only linear `n`/`p` stepping through them one at a time.
- **Solution:** A narrow sidebar or scrollbar-adjacent strip marking change locations by color
  (insert/delete/update/move), similar to what many GUI editors show next to the minimap/scrollbar.
- **Files:** New widget, `components/diff_viewer.rs` (layout), `theme.rs` (reuse existing diff
  colors).
- **Impact:** Meaningfully faster orientation on large diffs; the most visually involved item here.
- **Complexity:** Medium-High.

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
