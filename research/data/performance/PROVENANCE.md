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

## `robustness_full_summary.json` and `robustness_full_exceptions.csv` (2026-09-19)

The paper's Robust target exercised on the whole Full corpus: every modified code file in the
corpus's recent history - the population `../corpus_stats/edit_shape.csv` summarises - pushed
through `diff_code` with no node cap. Asked for by the 2026-09-18 review of the introductory paper
(R48 in `../../papers/introductory-paper/REVIEW-2026-09-18.md`).

| | |
|---|---|
| Corpus | `/var/tmp/research/full/repositories`, `DEPTH=50`, fetched 2026-08-20 |
| Pair list | `analysis/list_code_edits.py`: `git log --no-merges --numstat --diff-filter=MR -n50` per repository, shallow-boundary commits skipped, code files only; 444,199 pairs in 25 languages |
| Driver | `benchmark_diff_pairs --max-combined-nodes 1000000000 --timeout-secs 120 --iterations 1`, 8 processes, each in a systemd scope capped at 6 GB (`measure/overnight_benchmarks.sh`) |
| Then | `measure/r48_retry_killed.sh`: every harness-caused kill and every timeout re-run on a quiet machine, and one commit of each memory-killed file under 24 GB with a 300 s budget |
| Wall clock | main run 09:27–13:15 on 2026-09-19 (after two false starts, below); retry pass 13:17–14:29 |
| Result | 442,530 attempted; **442,322 completed (99.95%)**, 0 panics, 96 past the budget, 112 past the memory cap; 1,669 unreadable |

The merged per-pair CSV (444,199 rows) lives at `/var/tmp/research/full/robustness/robustness_full.csv`
and is not committed. `robustness_full_summary.json` is its aggregate (`analysis/robustness_merge.py
--summary-json`) and is what `analysis/paper_variables.py::robustness_full` derives the
`RobustnessFull*` macros from; `robustness_full_exceptions.csv` is every row that did not complete,
which is where the finding is.

### What did not complete

* **Unreadable (1,669).** Files with an extension the lister maps to a language but codediff has no
  grammar for (`.zsh`, 60 in the first 50k), files that are not valid UTF-8, and a few blobs git
  could not serve. Never reached the diff; excluded from the rate.
* **Past the 120 s budget (96).** Median combined size 1.3 M AST nodes. The budget was applied with
  eight processes sharing eight cores. 21 of the 89 original timeouts were re-run four at a time on
  the quiet machine before those re-runs themselves hit the 6 GB cap: 19 timed out again and 2
  completed. The other 68 keep their original status; 7 more come from the 24 GB pass, where a
  memory-killed file's representative commit ran past 300 s instead.
* **Past the 6 GB cap (112 pairs, 20 distinct files after the retry; 28 before).** Every one but
  `ArashPartow-exprtk/exprtk.hpp` (one commit of a hand-written 40,000-line single-header library,
  whose other commits complete in seconds) is generated or embedded content: tree-sitter
  `src/parser.c` parse tables (15 files), Intel metrics-discovery codegen, `glib-compile-resources`
  output (`mednaffe/src/resources.c`, 29 MB), Prodigal's `training.c`, brotli's test data, plotly's
  minified bundles, a Cython-generated `mpv.c`, a PNG as a C array. Under 24 GB and 300 s, one
  commit of each of the 28: 11 completed (the slowest, a tree-sitter TypeScript parser, in 164 s),
  16 ran past 300 s, and `mednaffe/src/resources.c` needed more than 24 GB.
* **Panics: none.** The harness's own aborts (below) were the harness.

Until 2026-09-20 the corpus statistics parsed a file only up to 1 MiB, so Table 1's maximum was
905,004 AST nodes while this run completed pairs of 7,092,498 a side. The cap is gone and the
4,014 files above it re-measured (see `../corpus_stats/PROVENANCE.md`); the corpus maximum is
23,584,040 nodes, and the two numbers now measure the same thing.

`peak_memory_bytes` is the diff thread's own heap as counted by the harness's allocator, not the
process's resident set; the killed pairs show that the two differ by an order of magnitude on the
largest inputs, so quote it for the completed pairs only (p99 35 MB, max 1.9 GB).

### The two false starts, and what they fixed

The first launch (05:21) ran the eight shards unconfined. Four minutes in, one shard's pair needed
more memory than the machine has, the kernel OOM-killed it, and systemd stopped the whole scope
the launching shell lived in - the other shards, the orchestrator and the Claude Code session that
had started them (`setsid` leaves the cgroup alone). Since then the chain runs as its own unit
(`systemd-run --user --unit codediff-overnight`), every shard in its own capped scope inside
`codediff-r48.slice`, and a killed shard resumes from its own output with the offending pair
recorded with the exit status it died with.

The restarts then surfaced three harness bugs, each fixed before the run that counts:

1. `benchmark_diff_pairs` ran its diff thread on the default 2 MB stack and aborted (SIGABRT) on
   inputs the product diffs on its 256 MB one - e.g. an ffmpeg codebook header of 69,403 nodes,
   fine in 452 ms on the right stack. It now uses `tui::app::DIFF_COMPUTE_STACK_SIZE`. Every
   exit-134 record was re-measured.
2. It kept every repository it had opened; after a few thousand the process ran out of file
   descriptors and recorded every later pair as unreadable (217,714 such rows, purged). The cache
   is now bounded (`OPEN_REPOS_MAX`).
3. `list_code_edits.py` mis-spelled the before-side path of a rename into or out of a directory
   (`a/{ => b}/c` became `a//c`), 6,675 pairs that read as unreadable. Fixed, the list regenerated,
   and the rows keyed by the mis-spelled paths (351) dropped and re-measured.

An unreadable pair now gets a row like every other outcome, which is what lets the resume logic
name the pair a killed attempt died on.
