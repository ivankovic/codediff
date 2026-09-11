# Provenance

Measured against the PRE-2026-08-18 sampled corpus (byte-size strata: small/medium/large/xlarge in
the `size_bucket` column). `../samples/` has since been re-drawn under the LOC buckets
(`stats::sampling::LOC_BUCKETS`), so re-running `measure/benchmark_all_extended.sh` now measures a
DIFFERENT pair set than these files - do not mix rows across that boundary. `baselines/` snapshots
are pinned to whatever corpus was current at their date; that is their point.

## `robustness_fixtures.csv` (2026-09-11)

The paper's robustness run, over the fixture corpus rather than a sampled pair set. Written by
`make measure-robustness-fixtures` (`benchmark_diff_pairs --fixtures --max-combined-nodes
1000000000 --timeout-secs 120 --iterations 5`) on the machine the paper's `MACHINE` block
describes. Same column schema as the `benchmark_<language>.csv` files above, but the naming
columns mean something else: `repository` is the dataset directory under `src/test/data/diffs/`
(`small`, `full` or `stratified` - the paper's datasets; `handmade` and `defects4j` are not
walked), `path` is the fixture directory name, and `size_bucket` and `commit` are empty.

Result: 942 fixture directories on disk that day, every one `ok` - no timeout, no panic, nothing
skipped (the node cap was set high enough to be inert). Largest input
`json-ipfs-ipfs-desktop-only-update-version-strings` at 198,406 nodes a side; slowest
`rust-rustdesk-rustdesk-actual-logic-change-in-io-loop-medium-sized-file` at 1,447 ms median;
peak thread heap 56.6 MB. `analysis/paper_variables.py::robustness_fixtures` scopes these rows
to the 775-fixture corpus frozen on 2026-09-09 (the `solution` column of
`../comparison/benchmark_accuracy.csv`); the 167 fixtures solved after the freeze are in the CSV
but not in the paper's macros. Every scoped figure above is unchanged by the scoping - the
extremes all fall inside the frozen 775.

Supersedes `robustness_rust.csv` (2026-08-20: 925 sampled Rust pairs, 16,000-node cap, 377 of
them unreadable because their clones' histories had been rewritten), which stays on disk as the
record of that earlier run but feeds nothing any more.
