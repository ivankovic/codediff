Performance fixture: real-world Rust file pair from tauri-apps/tauri
(examples/api/src-tauri/build.rs, commit a16796a55592cf5be80043edfbb630dd2e32efab) that takes
diff_code roughly 650ms despite a combined AST size of only 907 nodes. Captured via
research/diff_pairs_benchmark_rust.csv; useful for regression-testing and profiling
diff_code's worst-case behavior on small/medium real-world inputs.
