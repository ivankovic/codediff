Performance fixture: real-world Rust file pair from mozilla/firefox
(gfx/wr/webrender/src/prim_store/borders.rs, commit df7d2a75c2957f99765a015f6cfbd3ba6ef36ec9) that
takes diff_code roughly 840ms on a combined AST of 4916 nodes (~11.6KB per file) - notably faster
relative to its size than the other large fixtures here, included as a less extreme large-size data
point. Captured via research/diff_pairs_benchmark_rust.csv.
