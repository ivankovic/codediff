# Completed

All findings from the last review pass have been addressed:

*  Documented the `unsafe transmute` invariant in `NodeCache` (`diff.rs`) with a safety comment
   and explicit type annotations, rather than redesigning it - the type system still doesn't
   enforce the invariant (that the cache can't outlive the `Code` it was built from), so any
   future caller stashing a `NodeCache` somewhere longer-lived needs to read that comment.
*  Documented (and added a test pinning the current behavior of) the arbitrary-pick logic in
   `solve_identical_trees.rs` for duplicate-hash nodes: when N>1 before-nodes share a hash with
   N>1 after-nodes, they currently all collapse onto the *same* after-node rather than being
   distributed across the candidates - the heuristic itself was deliberately left unchanged
   (it affects diff quality across the whole dataset, not a cleanup).
*  Fixed the weak test in `code/metadata.rs` that claimed to check `reference_nodes_ordered`'s
   descending-size ordering but only checked hash-map membership; it now actually asserts the
   ordering.
*  Added tests for previously-untested real logic: `tui/ui.rs`'s `map_crossterm_event`,
   `tui/app.rs`'s `panic_message` and the file-selection/dialog-cancel state transitions, and
   `stats/filesystem.rs`'s `all_files_from_path` (using `tempfile`, no mocks).
*  Removed 5 unused dependencies (`tokio-util`, `fancy-regex`, `tree-sitter-cli`,
   `tree-sitter-haskell`, `tree-sitter-markdown`) and the dead `[build-dependencies]` section
   (no `build.rs` exists).
*  Ran `cargo clippy --fix` for the mechanical lints (needless_borrow, useless_vec,
   collapsible_if, or_insert_with, for_kv_map, derivable_impls) and hand-reviewed every changed
   line; all changes were confined to test code or behavior-preserving idiom swaps.
*  Deleted `gted_forced_left` (dead code): its own doc comment explained it was superseded
   scaffolding from before the real bidirectional `gted` existed, unlike its still-used sibling
   `gted_forced_right`. Hoisted two nested helper fns out of `spf_a` where their `pub(crate)` was
   a no-op (nested fn items can't be reached from outside their enclosing function regardless of
   visibility).
*  Silenced the 3 remaining `too_many_arguments` warnings (`optimal_iud.rs`) with
   `#[allow(clippy::too_many_arguments)]`, matching the existing convention used elsewhere in the
   algorithm core, rather than restructuring hot-path signatures.
*  Fixed the exploratory-testing bug where the "after" side's first node stayed highlighted blue
   forever regardless of cursor movement: `overlay_row` (`tui/widgets/code_viewer.rs`) painted
   each side's own `cursor_row`/`cursor_col` match unconditionally, but only the focused side's
   cursor is actually live - the unfocused side's was just wherever `load_ranges` had left it.
   Added `CodeViewerState::is_focused`, set by `DiffViewer::sync_focus` whenever `active_panel`
   changes (on load and on `Tab`), and gated both highlight mechanisms on it: the focused side
   shows its own cursor, the unfocused side shows only the destination pushed from the focused
   side's cursor - never both on one panel. Verified interactively in a real terminal (tmux) that
   the highlight now correctly follows the live cursor after `Tab` and after cursor movement.
*  Fixed the exploratory-testing complaint that the diff/cursor overlay colors are too dark on a
   light terminal, with a `c` theme picker rather than just retuning the one hardcoded palette:
   - Replaced the hardcoded RGB consts in `tui/widgets/code_viewer.rs` with `OverlayTheme`
     (`tui/theme.rs`), an enum of palettes - `Dark` (the original colors, kept as default),
     `Solarized Dark`, and `Solarized Light`. The Solarized variants aren't invented colors: each
     band is the real Solarized (Ethan Schoonover) accent alpha-blended toward that variant's own
     Solarized base color. `Solarized Light` is the actual fix - dark text on light pastel bands,
     instead of the light-on-dark scheme that's unreadable once a terminal's background is light.
   - `c` opens a popup (`tui/components/theme_dialog.rs`) listing all themes over the still-visible
     viewer; arrow keys + Enter apply the choice live to both panels via
     `DiffViewer::set_overlay_theme`.
   - Persisted to `.codediff.toml` in the cwd via `confy`, not `config-rs` (the literal
     most-downloaded Rust config crate): `config-rs` is read-only and can't write a choice back to
     disk, which storing a user choice requires. `confy::load_path`/`store_path` round-trip a
     small struct to an exact path, which is the actual shape of this problem.
   - Note for future work: syntax highlighting (`syntect`, `CodeViewerWidget::enable_syntax_
     highlighting`) is fully wired but never actually turned on anywhere in the live app - only in
     tests. Left untouched here (out of scope, and turning on dead code is pure regression risk),
     but worth knowing if "the code has no syntax highlighting" comes up in a future review.
   - Verified interactively in a real terminal (tmux): selecting "Solarized Light" changes both
     panels' colors immediately, writes the theme to `.codediff.toml`, and a fresh run of the
     binary loads that choice back on startup.

# Pending

*  `diff/text.rs`'s `TextDiff::for_range` is still an `unimplemented!()` stub. The TUI needed
   the same point-lookup query and ended up with its own (tested) binary-search implementation in
   `tui/widgets/code_viewer.rs` instead. Deliberately *not* consolidated in this pass: `for_range`
   stub's signature is an overlap query (`Vec<RangeMatch>`), while the TUI's is a single-point
   index lookup - forcing one to serve both isn't a clean fit, and refactoring the TUI's
   just-verified cursor lookup for marginal de-duplication value isn't worth the regression risk.
   Worth revisiting on its own, isolated from other work.
*  Two `// TODO: first pointers can be precomputed` notes remain in the APTED hot loop
   (`apted/engine.rs`) - a known micro-perf opportunity, not actioned since it's hand-tuned
   numerical code that should be profiled (via `benches/diff_code_benchmark`) before touching.
*  `diff/apted/common.rs` (2294 lines) and `engine.rs` (2219 lines) remain by far the largest
   files in the repo. Not a problem on its own; noted for context if more work lands there.
