# Small-but-slow diff_code examples

Extracted from `research/diff_pairs_benchmark_rust.csv` (status=ok, elapsed_ms >= 100): among pairs
that took at least 100ms, these are the 5 smallest by combined AST node count
(`ast_nodes_before + ast_nodes_after`). Re-measured directly against `diff_code` (3 runs each,
release build) to confirm the slowness is real and not a one-off CSV sample.

| # | dir | repository | path | nodes (before+after) | bytes (before+after) | csv elapsed_ms | re-measured elapsed_ms (median of 3) |
|---|-----|------------|------|----------------------|-----------------------|----------------|----------------------------------------|
| 1 | `1_rust_bootstrap_setup_tests` | rust-lang-rust.git | src/bootstrap/src/core/build_steps/setup/tests.rs | 322 | 1130 | 369.3 | 375.3 |
| 2 | `2_tauri_monitor_linux` | tauri-apps-tauri.git | crates/tauri-runtime-wry/src/monitor/linux.rs | 377 | 1310 | 109.5 | 113.0 |
| 3 | `3_tauri_helloworld_main` | tauri-apps-tauri.git | examples/helloworld/main.rs | 415 | 1465 | 113.1 | 118.3 |
| 4 | `4_sniffnet_packet_filters_fields` | GyulyVGC-sniffnet.git | src/networking/types/packet_filters_fields.rs | 449 | 1733 | 105.2 | 107.8 |
| 5 | `5_rust_issue_90762` | rust-lang-rust.git | tests/ui/consts/issue-90762.rs | 677 | 1612 | 233.6 | 241.9 |

For context, the fitted trend across all 543 "ok" benchmark pairs is roughly
`log(ms) ≈ 1.76 * log(nodes) - 9.27`, which predicts ~1-5ms for inputs this size. Each of these 5
pairs is 20-300x slower than that trend, despite being some of the smallest "slow" pairs (well
under 1.5KB per file) in the whole 1000-pair sample. They are good minimal-ish starting points for
debugging the Zhang-Shasha matching cost on pathological AST shapes (e.g. repeated/near-identical
sibling subtrees), without needing to wade through multi-KB files.

Each subdirectory has `before.rs` / `after.rs`, the exact blobs from `<commit>^:<old_path>` and
`<commit>:<path>` respectively, so they can be fed straight into `diff_code` (e.g. via
`Code::from_string`) for profiling.

## Source commits

1. rust-lang-rust.git @ a4d23841001ace656ed84df9792f257830f44fa3
2. tauri-apps-tauri.git @ 251203b8963419cb3b40741767393e8f3c909ef9
3. tauri-apps-tauri.git @ cedb24d494b84111daa3206c05196c8b89f1e994
4. GyulyVGC-sniffnet.git @ a1fef06bf732f952f5ee437270f05b2c1ae01461
5. rust-lang-rust.git @ a893257ceab1ea549112935c83c979daa123ebb8

## Method

Not delta-debugged/minimized — these are the original file pair contents as they appeared in the
sampled commits, picked purely by smallest AST size among pairs clearing the 100ms bar. Shrinking
them further to a minimal reproducer is a natural follow-up if needed.
