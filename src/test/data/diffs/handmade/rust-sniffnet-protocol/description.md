Performance fixture: real-world Rust file pair from GyulyVGC/sniffnet
(src/networking/types/protocol.rs, commit 2d9c9a06482ff40b759501ba53c1caf73b80e097) that takes
diff_code roughly 800ms despite a combined AST size of only 895 nodes (~25-100x slower than the
log-log size/time trend fitted across the broader benchmark sample predicts). Captured via
research/diff_pairs_benchmark_rust.csv; useful for regression-testing and profiling
diff_code's worst-case behavior on small/medium real-world inputs.
