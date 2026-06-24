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
