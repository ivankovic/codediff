# Provenance

`apted_only_group*.csv` hold the **2026-09-19** measurement, run unattended by
`measure/overnight_benchmarks.sh` (build -> R22 -> R48), whose R22 stage is stages 3 and 4 of
`measure/overnight_rq1_refresh.sh` (verify -> measure -> rebuild the paper) against a sample that
script's stage 2 had drawn the evening before at `COUNT=1000`.

| | |
|---|---|
| Corpus | `/var/tmp/research/full/repositories`, fetched at `--depth=50` on 2026-08-20, unchanged since |
| Sample | `../samples/sampled_code_pairs_*.csv`, drawn 2026-09-18 23:39 at `COUNT=1000` (142 per LOC bucket) |
| Pairs sampled | 19,798 across 24 languages from 3,256 repositories; every one resolved (`verify_sample.py`) |
| Pairs measured | 19,798, of which 9,311 completed inside the 1s budget and 10,487 timed out |
| Other statuses | 0 out of memory, 0 worker errors, 0 parse failures, 0 unreadable |
| Measurement | serial, 2026-09-19 01:26 to 05:21, nothing else running |
| Worker | `apted_only_worker` with `Algorithm::AptedWholeTree` (see below) |

It replaces the 2026-08-21 measurement (2,922 pairs at `COUNT=140`, 22 of 24 languages, 54.5% of
code pairs completing), which is in git history at `0c18cf7a`.

## What the worker measures, and the night it did not

RQ2 asks about *a single, whole-tree* tree-edit-distance computation. `apted_only_worker` calls
the engine's `for_roots`, and by 2026-09-18 that path no longer ran the kernel on a whole file:
`resolve_forest` settles any node with 50 or more children by a Myers pass over them (present in
August too), and since 2026-09-02 (`16399838`) decomposes any single pair above 600,000 cells
instead of running APTED on it. The first R22 pass of 2026-09-18/19 measured that engine: on the
308 pairs it shared with the August sample, 77 of the 84 August timeouts completed, a 1,182-node
file that had timed out took 0.5 ms, and group 1 came back with 5% timeouts against August's 44%.
That pass was stopped and discarded.

The measurement above uses `Algorithm::AptedWholeTree` (added 2026-09-19), which bypasses every
resolver shortcut - no byte-identical emit, no flat-container pass, no wrapper decomposition, no
cell gate - and hands the two roots to the kernel as they are. The driver also gained an
`out_of_memory` status for a worker that dies allocating the delta matrix; `apted_only_report.py`
counts it as a non-completion (`ATTEMPTED_STATUSES`), where a `worker_error` is excluded. None
occurred: Linux overcommit lets even a multi-hundred-gigabyte matrix allocate lazily, and the
1-second kill lands first.

The August numbers were therefore not whole-tree either: an 81,292-node Go file "completed" in
30 ms then, which the kernel cannot do. They were APTED after a Myers pass over the file's
top-level children. The paper's RQ2 text describes what this run measures, not that one.

## Read this before re-measuring

**The sample resolved completely, and that is not the normal state.** A sample CSV holds
`(repository, commit, path)` pointers, and the blobs are read out of the checkouts at measurement
time, so a sample is valid only while the history it points into is still present. Shallow clones
that get re-fetched drop old commits continuously.

The previous sample had decayed to **41% unreadable** by 2026-08-20, and the damage was not spread
evenly: `zed-industries-zed` had lost 207 of 208 sampled pairs and `vercel-next.js` 119 of 120,
while `rust-lang-rust` had lost none. Measuring it would have produced a healthy-looking output
that silently excluded whole projects. Run `analysis/verify_sample.py` after drawing a sample and
before measuring against it; it exits nonzero when anything fails to resolve, and reports the
per-repository breakdown rather than only an aggregate, because concentration is the part that
matters.

**Do not lower `DEPTH` on a re-fetch.** `git fetch --depth=N` shortens an existing shallow clone as
well as deepening it, so re-fetching below the depth a sample was drawn at destroys that sample's
resolvability in place.

## Sample size

`COUNT` (pairs per language, split over the 7 LOC buckets) is a parameter of
`measure/overnight_rq1_refresh.sh`, not a constant in it, since 2026-09-18 - the paper review asked
for 1000 per language, roughly 24,000 pairs, against the 140 (20 per bucket) this measurement used:

    cd research && COUNT=1000 SKIP_FETCH=1 ./measure/overnight_rq1_refresh.sh

`SKIP_FETCH=1` skips stage 1. That is sound when the sample is being **re-drawn**, because stage 2
reads the checkouts themselves and every pair it names therefore resolves by construction, and
stage 2b proves it before stage 3 measures anything. It is never sound when re-measuring an
existing sample - that is the decay case the whole script exists for, and the fetch is what fixes
it.

## Coverage

All 24 languages are in this measurement. The 2026-08-21 run covered 22: `Makefile`'s
`LANGUAGES` and the four `measure-apted-budget` groups then lacked R and Scala, which was fixed
the same day, and this is the first measurement since. R (110 pairs) and Scala (214) are the two
languages the corpus could not supply 1,000 pairs of.
