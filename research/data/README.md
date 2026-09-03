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
  comparison/     output  - codediff against other diff tools
  measure-rq1/            output  - whole-tree APTED against a 1-second budget (the paper's RQ1)
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
(`code_percentiles.csv`, the source of the introductory paper's Table 1), the AST node-kind
distribution per language, and size/LOC-changed statistics for the sampled Rust pairs. Written by
`analysis/file_stats.py` and `analysis/code_pair_diff_stats.py`.

`edit_shape.csv` is how big a real-world *edit* is, per language, over the most recent 50 commits
of each repository (`make measure-edit-shape MODE=<mode>`, `analysis/edit_shape_stats.py`) - the source of
the paper's Table 2. Per-language rows only: the per-edit population is ~48k modifications and the
uncapped one ~20M, neither of which belongs in git. The 50-commit cap is load-bearing rather than
a speed measure - these clones are shallow but not uniformly so, and `torvalds-linux.git` alone
carries 1.29M of the corpus's 2.31M reachable commits, so an uncapped walk measures the Linux
kernel and calls it the corpus.

## quality/

How well the diff algorithm reproduces the human-authored ground-truth mappings in
`src/test/data/diffs/`. `optimal_solutions_benchmark.csv` is the per-fixture mismatch count,
`human_mapping_analysis.csv` the shape analysis of the mappings themselves, and
`quality_baseline.txt` the pinned numbers `make check-quality` (and therefore `make deploy`) gates
against. That last file is read from the *root* Makefile - it is a release gate, not a research
artifact, and is only filed here because it describes the same measurement.

## performance/

`benchmark_<language>.csv` plus the `_sample.log` / `_benchmark.log` files from the run that
produced each, written by `measure/benchmark_all_extended.sh`. The logs are kept deliberately:
they are the only record of why a given language sampled fewer pairs than expected (see that
script's own comment about Lua sampling zero pairs).

`baselines/` holds dated or labelled snapshots (`benchmark_2026-08-17_after_runtime_pass.csv`,
`benchmark_<language>_baseline_pre_<change>.csv`) kept for before/after comparison across a
specific algorithm change. These are never regenerated - that is the point of them.

## measure-rq1/

`apted_only_group<N>.csv`, the per-pair output of `apted_only_benchmark`: whether a single
whole-tree APTED run finished inside a 1-second budget, for every sampled pair. Four groups purely
so each file (and each restart, if a run is interrupted) stays a manageable size; they are measured
serially, never in parallel, because the measurement is wall-clock against a fixed budget.

Also holds `archive_pre_resample_*/` (the last measurements taken against the older, byte-bucket
corpus) and, currently, `partial_resample_*_INCOMPLETE/` - see that directory's own README for why
it is not reportable.

Read with `make rq1-report`; re-measure with `make measure-rq1` (hours, needs an idle machine).
