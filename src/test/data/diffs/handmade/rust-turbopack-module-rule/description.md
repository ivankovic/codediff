Performance fixture: real-world Rust file pair from vercel/next.js
(turbopack/crates/turbopack/src/module_options/module_rule.rs, commit
b067991e591f4965b873d4eef9a18ea1f321ca9b) that takes diff_code roughly 2.5 seconds on a combined
AST of 3925 nodes (~8.5-11KB per file). Captured via research/diff_pairs_benchmark_rust.csv; useful
for regression-testing and profiling diff_code's worst-case behavior on larger real-world inputs.
