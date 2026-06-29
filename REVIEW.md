# Completed

*  Bug fix: inactive panel viewport now scrolls to track the active panel's cursor destination
   (`tui/components/diff_viewer.rs` — `sync_scroll`, called from both `move_cursor_*` methods;
   `tui/components/code_viewer.rs` — new `scroll_to_show_row` helper).
*  `diff/text.rs`'s `TextDiff::for_range` implemented as a linear-scan overlap filter using the
   existing `TextRange::intersects`. The TUI's binary-search point-lookup (`tui/widgets/code_viewer.rs`)
   is intentionally kept separate: the two queries have different shapes (overlap vs. point) and
   different callers, so merging them would just add accidental coupling.

# Pending

*  Two `// TODO: first pointers can be precomputed` notes remain in the APTED hot loop
   (`apted/engine.rs`) - a known micro-perf opportunity, not actioned since it's hand-tuned
   numerical code that should be profiled (via `benches/diff_code_benchmark`) before touching.
*  `diff/apted/common.rs` (2294 lines) and `engine.rs` (2219 lines) remain by far the largest
   files in the repo. Not a problem on its own; noted for context if more work lands there.
