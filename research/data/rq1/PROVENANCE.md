# Provenance

`apted_only_group*.csv` hold the 2026-08-21 measurement, run unattended by
`measure/overnight_rq1_refresh.sh` (fetch -> sample -> verify -> measure -> rebuild the paper).

| | |
|---|---|
| Corpus | `/var/tmp/research/full/repositories`, fetched at `--depth=50` on 2026-08-20 |
| Repositories | 7,445 listed, 121 (1.6%) failed to fetch, 1,224 contributed a sampled pair |
| Sample | `../samples/sampled_code_pairs_*.csv`, re-drawn 2026-08-21 at `COUNT=140` |
| Pairs sampled | 3,089 across 24 languages, 20 per (language, LOC bucket) where the corpus has them |
| Pairs measured | 2,922, of which 1,637 completed inside the 1s budget and 1,285 timed out |
| Unreadable | **0** |

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

## Known gap in this measurement

**2,922 of 3,089 sampled pairs were measured.** The missing 167 are R (79) and Scala (88): the
sampler drew them, but `Makefile`'s `LANGUAGES` and the `measure-apted-budget` target's four group lists covered
only 22 of the 24 languages, so no per-language CSV existed for either. Both were added to the
Makefile on 2026-08-21 and their CSVs generated from `sampled_code_pairs_all.csv`, so the next
`make measure-apted-budget` covers all 24 — but the numbers currently committed here, and the ones the paper cites,
are the 22-language measurement. Do not describe this measurement as covering the whole sample.

## Superseded data

* `archive_pre_resample_2026-08-18/` - the last measurement against the older, byte-bucket corpus.
  Its numbers are what the paper cited before 2026-08-21.
* `partial_resample_2026-08-18_INCOMPLETE/` and `partial_resample_2026-08-20_INCOMPLETE/` - two
  stopped runs, kept only so a resumed run need not restart from zero. Each covers a
  non-representative slice of one group; neither is reportable. See their own READMEs.
