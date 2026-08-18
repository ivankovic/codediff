# Introductory paper

An ACM `sigconf`-format LaTeX paper, "CodeDiff: A Fast, Robust, Production-Ready, Syntax-Aware
Code Diffing Tool". Introduces CodeDiff itself as a production pipeline built from established
tree-edit-distance algorithms (APTED, GumTree, Myers diff, with Zhang-Shasha as an internal
correctness oracle), backed by:

* an empirical study of real-world repository, file, and AST size, motivating CodeDiff's concrete
  speed and robustness targets instead of an assumed worst case
* a per-heuristic ablation study (real numbers from `src/diff.rs`'s `HeuristicConfig` doc comment
  and `research/data/quality/optimal_solutions_benchmark.csv`), showing that one of GumTree's own two extra
  heuristics helps CodeDiff's pipeline and one measurably hurts it
* a head-to-head accuracy, speed, and robustness comparison against GumTree v4.0.0-beta8, Unix
  `diff`, difftastic, and diffsitter, on 98 real-world fixtures with human-verified ground truth
  (real numbers and charts from `research/data/comparison/benchmark_other.csv` and
  `research/analysis/benchmark_other_report.py`'s output)

The comparison is written to be read as generous, not competitive: difftastic and diffsitter
optimize for human readability rather than ground-truth mapping fidelity, and GumTree's own
evaluation, on its own pipeline and corpus, is what motivated adopting its move-detection
heuristic in the first place - Section 6 and the Conclusion both frame CodeDiff's numbers as one
way of combining what each tool already does well, not as a claim that any of them are lesser.

## Status

Compiles cleanly, 6 pages, builds with `latexmk -pdf -g main` (verified locally, `cm-super` +
`texlive-publishers` installed). See the `TODO` comments in `main.tex` for the placeholder ACM
conference/rights metadata and CCS concepts, still to fill in once a venue is chosen.

### Every number is a macro

**`main.tex` contains no bare numeric literal that came from a measurement.** Every number in the
paper - prose, tables, and abstract alike - is a LaTeX macro defined in the single generated file
`figures/variables.tex`, which `main.tex` `\input`s exactly once. Regenerate it with `make
paper-variables` (fast; reads only artifacts already on disk), or via either paper target below,
which run it for you.

This matters because several numbers appear more than once - the fixture count appears nine times,
the node accuracy three, and each line-level mismatch rate appears in both Table 3 and the prose
discussing it. When those were literals, a data refresh could update one occurrence and miss
another. A single macro used in every position cannot drift.

`research/analysis/paper_variables.py` assembles that file and documents, per block, where each
number comes from. Two populations, kept visibly distinct in the generated output:

* **Generated** - read back from a measurement artifact. Currently the empirical-study block
  (corpus size, size percentiles, bytes-AST correlation), which
  `file_stats.py`'s `write_paper_variables` writes to `plots/variables_empirical.tex` from
  `stats.sqlite`. Refreshing is "re-run the producer, re-run the assembler."
* **Authored** - no saved producer to read back from, so the value is transcribed, but into one
  version-controlled place with a comment naming the command that produced it. This covers the
  ablation deltas, the per-tool comparison and speed tables, the robustness run, and the design
  targets. Wiring each to a real artifact is follow-up work; the assembler records what that
  artifact would be.

**Edit authored values in `paper_variables.py`, never in `figures/variables.tex`.** That file is
generated output: `make introductory-paper` regenerates and overwrites it on every routine PDF
rebuild, so a hand-edit there survives only until the next build. The authored numbers live in the
`CORPUS` / `ABLATION` / `COMPARISON` / `SPEED` / `ROBUSTNESS` / `TARGETS` dicts near the top of the
script.

**`figures/` contains symlinks, not copies.** Every entry in `figures/` points at
`research/plots/`, which is the single source of truth for generated figures and for
`variables.tex`. There is no copy step: regenerating a chart or the macro file updates what the
paper builds from, immediately. `research/plots/` is committed for exactly this reason.

If the empirical fragment (`plots/variables_empirical.tex`, written by the slow `file-stats` run)
is missing, the assembler carries the previous values forward from the `variables.tex` already on
disk rather than overwriting good numbers with placeholders - `file-stats` is deliberately not a
prerequisite of the paper targets, so a routine rebuild must not destroy them. It only emits a
loud `\textbf{??}` when a macro has no value from any source - never silently omits one, because
`main.tex` builds under `-interaction=nonstopmode`, where an *undefined* macro does not fail the
build; it just yields a PDF with the number quietly missing.

### Known-stale numbers

**The empirical-study numbers (Section 3, Table 1, corpus size) currently reflect the 100-repository
`small` sample, not the paper's eventual full 7,423-repository corpus.** This is deliberate, not a
bug: computing the full corpus's numbers for real means running `file_stats` (tree-sitter parsing
+ AST-node counting) over ~7,445 already-cloned repositories, which a timing probe against the
100-repo sample put at roughly 24-25 hours and ~400GB of database (measured 2026-07-31: 100 repos
took 19m50s, 1.35M files, 1,133 files/sec). Swapping in the real full-corpus numbers, once that run
finishes, is `make file-stats MODE=full` (slow, run once) followed by `make
introductory-paper-empirical MODE=full` (fast, re-renders and rebuilds) - no hand-editing required.

**The 98-fixture corpus and every number measured on it are also stale**, and by more than the
empirical block: `research/data/quality/optimal_solutions_benchmark.csv` now holds 433 rows and
`research/data/comparison/benchmark_other.csv` 156, against the paper's 98. That affects the AST-node accuracy, the
ablation table, the line-level comparison, and the speed percentiles. These must be refreshed
*together*, not piecemeal - they were all measured on the same corpus, so updating one block alone
would leave the paper internally inconsistent. The ablation table additionally describes a
"Flow-control arm matching" pass that was deleted from the codebase on 2026-08-14;
`research/measure/ablation_study.sh` now ablates three passes, not four.

### Why this mechanism exists

A real, already-happened failure: this paper's original Table 1 (before `variables.tex` existed)
had its numbers hand-transcribed from a conference slide deck
(`research/presentations/MUC-2026-03`), and that slide deck's own source computation was never
saved anywhere in this repository. By the time anyone asked why Bytes' and LOC's Max column was
blank, there was no way to answer it, and no way to regenerate the numbers except starting the full
pipeline from scratch. `write_paper_variables`'s own doc comment in `file_stats.py` tells this story
in full.

Everything else the paper embeds also traces to a file in this repository: the
accuracy/speed/robustness charts and the variance table to `research/data/comparison/benchmark_other.csv` and
`research/analysis/benchmark_other_report.py`'s output (`benchmark_other_accuracy.png`,
`benchmark_other_runtime.png`, and `benchmark_other_variance.tex` - the last is a generated LaTeX
table, not a chart, `\input` directly rather than copied as a PNG).
`figures/files_per_project.png` was dropped from an earlier draft after turning out to be a stale,
empty scratch artifact.

**The RQ1 measurement block is generated, not authored** (`plots/variables_rq1.tex`, written by
`analysis/apted_only_report.py` via `make rq1-report`), so a full `make rq1` re-measurement flows
into the paper with no hand-editing. The numbers currently on disk were measured against the
pre-2026-08-18 sampled corpus (see `data/rq1/PROVENANCE.md`); the prose cites only the code and
config/data categories, deliberately - the scripting category does not exist in that measurement
and its macros are not emitted until the re-measurement lands.

## Building

This uses the ACM `acmart` document class, which is not vendored into this repository. To build
locally you need a LaTeX distribution that includes it, plus `cm-super` for full font expansion:

* Debian/Ubuntu: `sudo apt-get install texlive-publishers texlive-latex-extra cm-super`, then
  `latexmk -pdf -g main` (or `pdflatex main && bibtex main && pdflatex main && pdflatex main`). The
  `-g` matters: without it, latexmk can decide `main.pdf` is already current from `main.tex`'s own
  timestamp and skip rebuilding even though an `\input`-ed generated table changed underneath it.
* Or use a `texlive/texlive` Docker image.

Note that `figures/` holds symlinks into `research/plots/`, so building requires a real checkout -
copying this directory alone, without dereferencing them, leaves every figure dangling.

Regenerating from `research/` (not the repository root - these targets live in
`research/Makefile`):

* `make introductory-paper` - fast (seconds). Re-renders the benchmark_other charts/table from
  whatever `research/data/comparison/benchmark_other.csv` already has, regenerates `plots/variables.tex`, and
  rebuilds the PDF.
* `make introductory-paper-empirical MODE=<tiny|small|full>` - fast (seconds), re-renders Table 1
  and friends from whatever `MODE`'s `stats.sqlite` already has, and rebuilds the PDF. Does *not*
  run `file_stats` itself - that's `make file-stats MODE=<mode>`, and it's the slow one (see
  "Status" above).
* `make paper-variables` - fast (instant). Just regenerates `plots/variables.tex` from whatever
  is already on disk, without rebuilding the PDF.
