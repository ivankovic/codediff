Performance fixture: real-world Rust file pair from tauri-apps/tauri
(examples/api/src-tauri/build.rs, commit cde0ff7798a46712f69b34aab952209f45500fe9, a later commit
on the same file as medium-tauri-api-build-1) that takes diff_code roughly 670ms despite a
combined AST size of only 909 nodes. Captured via research/diff_pairs_benchmark_rust.csv; useful
for regression-testing and profiling diff_code's worst-case behavior on small/medium real-world
inputs.
