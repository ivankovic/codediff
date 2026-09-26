# research/data

Every dataset this project measures over or produces. Organised by *what question each file
answers*, not by which tool happened to write it - a reader looking for "how accurate is the diff"
should not have to know that the answer lives in `optimal_solutions_benchmark.csv`.

Each measurement directory carries a `PROVENANCE.md` recording what its files were measured
against (which corpus draw, which fixture set) - read it before comparing numbers across
directories, since the sampled corpus and the measurements over it do not refresh atomically.

```
data/
  samples/        inputs  - which (repository, commit, path) pairs we measure over
  corpus_stats/   output  - descriptive statistics about the corpus itself
  quality/        output  - diff accuracy against human-authored ground truth
  performance/    output  - diff speed/memory over the sampled pairs
    baselines/            - pinned point-in-time snapshots, kept for comparison
  comparison/     output  - codediff against other diff tools, and against external oracles
  rq1/            output  - whole-tree APTED against a 1-second budget (the paper's RQ2, named RQ1
                            in files and macros)
  coverage/       output  - line coverage of the test suite: the README badge and per-test sets
  ablation/       output  - leave-one-out heuristic study (gitignored: regenerable scratch)
```

## samples/

`sampled_code_pairs_<language>.csv`, one per language, written by `make sample-pairs-all`. These
hold *pointers*, not content: `(language, size_bucket, repository, commit, path, old_path)`. The
before/after blobs are read back out of the checkouts under `/var/tmp/research/<mode>/` at
measurement time, so this directory stays small and the corpus stays reproducible.

`size_bucket` follows `stats::sampling::LOC_BUCKETS` (keyed by the larger of before/after LOC).
Note that CSVs written before 2026-08-18 carry the older byte-size labels (`small`/`medium`/
`large`/`xlarge`) in that same column.

`sampled_code_pairs_all.csv` is the combined pre-split intermediate and is gitignored - the
per-language files are the artifact.

## corpus_stats/

What the corpus *looks like*, independent of any diff: per-file size percentiles
(`code_percentiles.csv`), the whole per-file size distribution behind them
(`code_file_size_distribution.csv`, one `metric,value,count` row per distinct size - the source of
the introductory paper's corpus-shape figure, drawn by `analysis/distributions_report.py`), the
AST node-kind distribution per language, and size/LOC-changed statistics for the sampled Rust
pairs. Written by `analysis/file_stats.py` and `analysis/code_pair_diff_stats.py`.

`edit_shape.csv` is how big a real-world *edit* is, per language, over the most recent 50 commits
of each repository (`make measure-edit-shape MODE=<mode>`, `analysis/edit_shape_stats.py`) - the source of
the paper's edit-size numbers. Per-language rows only: the per-edit population is ~435k modifications
and the uncapped one ~20M, neither of which belongs in git. `edit_shape_distribution.csv`, written
by the same run, is the whole distribution in committable form - `metric,value,count` rows for
lines changed per file edit, lines changed per commit, files per commit and share of the file
rewritten (in permille) - and is what the paper's corpus-shape figure draws. The 50-commit cap is load-bearing rather than
a speed measure - these clones are shallow but not uniformly so, and `torvalds-linux.git` alone
carries 1.29M of the corpus's 2.31M reachable commits, so an uncapped walk measures the Linux
kernel and calls it the corpus.

## quality/

How well the diff algorithm reproduces the human-authored ground-truth mappings in
`src/test/data/diffs/`. `optimal_solutions_benchmark.csv` is the per-fixture mismatch count,
`human_mapping_analysis.csv` the shape analysis of the mappings themselves. Three files are
product gates rather than research artifacts, read by the *root* Makefile and filed here because
they describe the same measurement: `quality_baseline.csv`, the per-fixture accuracy baseline
`make check-quality` (and therefore `make deploy`) gates on; `quality_baseline.txt`, its runtime
baseline, which only warns; and `painting_attribution.csv`, the per-fixture painting baseline
`make check-painting-attribution` gates on. `PROVENANCE.md` lists the rest.

## performance/

`benchmark_<language>.csv` plus the `_sample.log` / `_benchmark.log` files from the run that
produced each, written by `measure/benchmark_all_extended.sh`. The logs are kept deliberately:
they are the only record of why a given language sampled fewer pairs than expected (see that
script's own comment about Lua sampling zero pairs).

`baselines/` holds dated or labelled snapshots (`benchmark_2026-08-17_after_runtime_pass.csv`,
`benchmark_<language>_baseline_pre_<change>.csv`) kept for before/after comparison across a
specific algorithm change. These are never regenerated - that is the point of them.

## rq1/

`apted_only_group<N>.csv`, the per-pair output of `apted_only_benchmark`: whether a single
whole-tree APTED run finished inside a 1-second budget, for every sampled pair. Four groups purely
so each file (and each restart, if a run is interrupted) stays a manageable size; they are measured
serially, never in parallel, because the measurement is wall-clock against a fixed budget.

Read with `make apted-budget-report`; re-measure with `make measure-apted-budget` (hours, needs an idle machine).

## coverage/

`badge.json` is the README's coverage badge, rewritten by the root Makefile's `make coverage`.
`per_test/` holds one line-coverage set per test, written by `make measure-per-test-coverage` in
`research/` and gitignored apart from its `PROVENANCE.md`.
