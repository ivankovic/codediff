# Open code-health items

- `bin/apted_only_benchmark.rs` and `bin/benchmark_diff_pairs.rs` both define `read_pairs_csv`,
  `open_repo` and `blob_content`; share them (the CSV writers must stay byte-identical).
- `test/helper.rs`: `was_node_added/deleted` and `was_tree_added/deleted` differ only in key order.
- `bin/human_solver/tests.rs` (9,275 lines): lift the shared fixtures into their own module, then
  move tests beside their code.
- `diff/apted/common/myers.rs` keeps a full `V` snapshot per step, each sized by the edit
  limit rather than the step: O(d × limit) memory.
- `std::HashMap`/`HashSet` (SipHash) remains in thirteen non-test `src/diff` files, and the
  `apted/common` submodules take it through `use super::*`; the engine's other maps are Fx.
- `ASTMappingReason::OptimalIDU` is never produced by a pass; drop it with the next benchmark CSV
  regeneration.
- No tests: `bin/benchmark_other/{bdiff,git,nvim}.rs`, `stats/git.rs`, `tui/events.rs`,
  `tui/actions.rs`.
