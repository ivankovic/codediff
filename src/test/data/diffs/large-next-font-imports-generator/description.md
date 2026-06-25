Performance fixture: real-world Rust file pair from vercel/next.js
(crates/next-custom-transforms/src/transforms/fonts/font_imports_generator.rs, commit
f93a867d7741214ed6544a0ff673f498ca69234b) that takes diff_code roughly 1.85 seconds on a combined
AST of 4761 nodes (~10-11KB per file). Captured via research/diff_pairs_benchmark_rust.csv; useful
for regression-testing and profiling diff_code's worst-case behavior on larger real-world inputs.
