Performance fixture: real-world Rust file pair from zed-industries/zed
(crates/git_ui/src/git_panel_settings.rs, commit 00bee4515e2c69fbcedcb7032420ce9dafce6aa7) that
takes diff_code roughly 2.4 seconds despite a combined AST size of only 1232 nodes. Captured via
research/diff_pairs_benchmark_rust.csv; useful for regression-testing and profiling diff_code's
worst-case behavior on medium-sized real-world inputs.
