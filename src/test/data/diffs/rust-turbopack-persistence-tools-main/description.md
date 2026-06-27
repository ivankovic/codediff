Performance fixture: real-world Rust file pair from vercel/next.js
(turbopack/crates/turbo-persistence-tools/src/main.rs, commit
11823f8471cec98a093fd3af6d4193838da0b667) that takes diff_code roughly 5 seconds despite a
combined AST size of only 925 nodes and ~2KB per file - one of the most extreme slowdowns found in
the benchmark sample (~300x the fitted size/time trend). Captured via
research/diff_pairs_benchmark_rust.csv; a strong candidate for profiling diff_code's worst-case
behavior.
