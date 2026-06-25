Performance fixture: real-world Rust file pair from tauri-apps/tauri
(crates/tauri-cli/src/mobile/ios/dev.rs, commit 0aa48fb9e4b9d7b5bf3522000a76ebc1836394ed) that takes
diff_code roughly 13 seconds on a combined AST of 5162 nodes (~10.3KB per file) - the single
slowest pair found across the entire benchmark sample. Captured via
research/diff_pairs_benchmark_rust.csv; the top priority candidate for profiling diff_code's
worst-case behavior.
