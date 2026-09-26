# research/

Everything that exists to produce CodeDiff's papers and empirical studies: the corpus, the
measurements over it, the analysis scripts, the generated figures, and the papers themselves. Part
of the project but not part of the product. The product's code does not read it; its test suite
and a few dev tools do. `src/test.rs` checks `data/quality/quality_baseline.csv` against the
fixture stubs, and the census tests and `src/bin/` tools write their outputs under `data/` (see
below).

## Layout, in pipeline order

```
sampling/        corpus acquisition: Gentoo package list -> repository list -> clones
data/samples/    which (repository, commit, path) pairs we measure over    } inputs
data/...         measurement outputs, grouped by question - see data/README.md
measure/         drivers that produce data/ (benchmarks, overnight runs, per-test coverage)
analysis/        Python reports: read data/, print tables, write plots/
plots/           every generated figure and variables.tex - single source of truth
papers/          LaTeX papers; figures/ symlinks into plots/, never copies
presentations/   conference decks (point-in-time artifacts)
drivers/         vendored external-tool harnesses (GumTree batch driver)
external/        fetch scripts for third-party ground truth we score against (never vendored)
```

The flow is one-directional: `sampling` fills checkouts under `/var/tmp/research/<mode>/`
(and `external/` fills `/var/tmp/research/external/` with other people's oracles), `measure/` and
the Rust binaries in `src/bin/` turn those into `data/`, `analysis/` turns `data/` into `plots/`,
and `papers/` builds from `plots/`. When something looks stale, walk upstream:
a wrong number in the paper is a `plots/` question, which is an `analysis/` question, which is a
`data/` question.

The repository lists (`list_of_repositories*.csv`) live at the repository root, not here: they
define which projects the corpus covers, which is referenced beyond research/.

## Running things

All research targets live in `Makefile` *in this directory* - the repository-root Makefile holds
product concerns only (build, test, install, release):

```
cd research
make apted-budget-report   # fast: re-render RQ2 (named RQ1 in files and macros) from data on disk
make introductory-paper    # fast: rebuild the paper PDF
make fetch MODE=small      # slow: clone/update the 100-repository corpus
make measure-apted-budget  # slow, timing-sensitive: re-measure RQ2 (idle machine!)
```

Most corpus targets take `MODE=tiny` (default) / `MODE=small` / `MODE=full`, choosing which
checkout set under `/var/tmp/research/` they operate on. `Makefile` in this directory is the full
reference - every target carries its own comment explaining what it measures and what it needs.

Python scripts run via `uv` using this directory's `pyproject.toml`/`uv.lock`; each is also
independently runnable (`uv run ./analysis/<script>.py`).

## Provenance discipline

Measurement outputs are only meaningful relative to the corpus and code version that produced
them. Each `data/` measurement directory carries a `PROVENANCE.md` saying what its files were
measured against - read it before comparing numbers across directories, and update it when a
re-measurement lands. The papers never hand-transcribe numbers: every figure and macro traces to
a file here (see `papers/introductory-paper/README.md`).

Three product-side files live in `data/quality/`, read by the *root* Makefile and filed with the
other quality data because they describe the same measurement: `quality_baseline.csv`, the
per-fixture accuracy baseline `make check-quality` (and therefore `make deploy`) gates on;
`quality_baseline.txt`, its runtime baseline, which only warns; and `painting_attribution.csv`,
the baseline `make check-painting-attribution` gates on. Both gates run in CI.
