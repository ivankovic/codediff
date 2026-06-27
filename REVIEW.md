# Completed

# Pending

*  Feature request: pressing 'g' moves the cursor on the inactive tab to the matched node, and then
   switches the active tab as if you pressed Tab.
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

