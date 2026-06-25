Performance fixture: real-world Rust file pair from zed-industries/zed
(crates/workspace/src/tasks.rs, commit 0698eccebb3ddd89e00081af3a8aab540478d220) that takes
diff_code roughly 1.3 seconds on a combined AST of 4075 nodes (before file 4.5KB, after file
11.7KB - a large insertion). Captured via research/diff_pairs_benchmark_rust.csv; useful for
regression-testing and profiling diff_code's worst-case behavior on larger real-world inputs.
