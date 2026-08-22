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

Compiles cleanly, 11 pages, builds with `latexmk -pdf -g main` (verified locally, `cm-super` +
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

* **Generated** - read back from a measurement artifact. Refreshing is "re-run the producer, re-run
  the assembler." Five blocks: the empirical study (`file_stats.py` -> `variables_empirical.tex`),
  RQ1 (`apted_only_report.py` -> `variables_rq1.tex`), the tool comparison
  (`benchmark_other_report.py` -> `variables_comparison.tex`), RQ3's ground-truth ambiguity
  (`ambiguity_report.py` -> `variables_ambiguity.tex`, via `make ambiguity-report`), and the
  change-shape census (`human_mapping_shapes_report.py` -> `variables_shapes.tex`, via `make
  shapes-report`) - the last currently generated but unreferenced, see the RQ1-RQ3 rule above.
* **Authored** - no saved producer to read back from, so the value is transcribed, but into one
  version-controlled place with a comment naming the command that produced it. This covers the
  corpus/node-accuracy totals, the ablation deltas, the robustness run, and the design targets.

The per-tool comparison and speed tables moved from Authored to Generated on 2026-08-20. At ~30
hand-transcribed numbers they were the largest authored group and the one every data refresh
touches, so they were the likeliest to drift. `benchmark_other_report.py` now derives them from
`benchmark_accuracy.csv` (accuracy, which carries an explicit per-tool `_status` column and is
machine-independent) and `benchmark_other.csv` (timing).

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

### RQ1-RQ3 are answered without reference to CodeDiff

Set on 2026-08-22, and it is a structural rule, not a stylistic preference: an RQ answer describes
the problem or the state of the art, so no RA box and none of the argument supporting one mentions
CodeDiff. Section 3 keeps one attribution - the RQ1 measurement names the APTED implementation it
ran, because that is what makes its "lower bound, not upper bound" caveat checkable - but states
it tool-neutrally. Three consequences that are easy to undo by accident:

* **The four-tool comparison excludes CodeDiff entirely.** Table 2 has four rows, and
  `benchmark_other_report.py::plot_accuracy` deliberately drops the `codediff` series (see its doc
  comment). `plot_runtime` keeps it, because speed is reported as CodeDiff's own production
  viability rather than as an RQ answer. The `\CodeDiffLineRate{}` family of macros is still
  generated and simply unused; that is intentional, so the number is one edit away if the framing
  changes back.
* **RQ3 was pivoted** from "which change shapes need a dedicated heuristic" to "when does a change
  have no single correct mapping". It now covers two phenomena that are deliberately kept apart,
  because they have different consequences and different epistemic status:
  * *Multiple optimal solutions exist* - several one-to-one mappings are equally correct, recorded
    as a `MultiMapGroup`. Measured, and reported as a rate, by `analysis/ambiguity_report.py`
    (`make ambiguity-report`).
  * *No one-to-one optimal solution exists* - the true correspondence is N:M, which ordered tree
    edit distance cannot express by definition and `human_mapping.json` cannot express either.
    Reported as an existence result with named instances and **no denominator**; the curated list,
    the grep that found it, and the two exclusions live in `data/quality/nm_instances.md`. Do not
    turn that list into a generated macro - the file explains why. The change-shape census that answered
  the old RQ3's first half was cut from the paper; `human_mapping_shapes_report.py`, `make
  shapes-report`, and the whole `Shape*` macro block still exist and still run - only `main.tex`'s
  references were removed.
* **The ablation study stayed but is no longer an RA.** It sits under CodeDiff's own evaluation,
  after RA3, reframed as a property of this pipeline rather than an answer about the field. Its
  "unique type matching fires zero times" result is unchanged and is still cited from the
  Conclusion's recommendations.

Two properties of `ambiguity_report.py` are load-bearing and easy to break. It scopes itself to
the fixture set in `data/quality/human_mapping_analysis.csv`, so Section 5 keeps describing one
corpus state. And it reads each fixture's annotation era from the committed
`data/quality/ambiguity_eras.csv` rather than re-deriving it from `git log` on every run: the
multi-mapping facility postdates part of the corpus, so the headline rate depends on that
classification, and without the committed record one formatting sweep over `src/test/data/diffs/`
would move every pre-facility fixture into "revisited" and change the rate with no corpus change
at all. `--refresh-eras` re-derives deliberately.

### Known-stale numbers

**The empirical-study numbers (Section 3, Table 1, corpus size) currently reflect the 100-repository
`small` sample, not the paper's eventual full 7,423-repository corpus.** This is deliberate, not a
bug: computing the full corpus's numbers for real means running `file_stats` (tree-sitter parsing
+ AST-node counting) over ~7,445 already-cloned repositories, which a timing probe against the
100-repo sample put at roughly 24-25 hours and ~400GB of database (measured 2026-07-31: 100 repos
took 19m50s, 1.35M files, 1,133 files/sec). Swapping in the real full-corpus numbers, once that run
finishes, is `make file-stats MODE=full` (slow, run once) followed by `make
introductory-paper-empirical MODE=full` (fast, re-renders and rebuilds) - no hand-editing required.

**Everything in Section 5 was refreshed on 2026-08-20** against the current 468-fixture corpus
(469 fixture directories, 468 of them carrying a `human_mapping.json`), replacing numbers measured
on 98 fixtures. Refreshed together, deliberately: AST-node accuracy, the ablation study, the
per-tool line-level comparison, the speed percentiles, and the robustness run were all measured
against the same corpus, so refreshing one block alone would leave the section internally
inconsistent. Keep that property on the next refresh.

That pass changed more than the values:

* **The ablation table is a different table.** All four passes it lists are different passes from
  the four it listed before. `ablation_study.sh`'s `FLAGS` array had gone stale against the binary
  (it named `solver-import-nodes` and `solver-bottom-up-expansion`, neither of which exists), and
  a stale flag makes clap exit before scoring a single fixture, which the script reported as a
  per-flag `FAILED` row rather than as the list being wrong. It now pre-flights every flag against
  `--help` and fails loudly instead.
* **The result inverted.** Three of the four passes now measurably help, where three of the old
  four were net-negative. Move-detection recovery is worth more than the other three combined.
* **Section 4's pipeline description was rewritten**, because the pipeline itself had changed: the
  whole-residual APTED call is gone (Myers-LCS runs unconditionally, and APTED survives only
  scoped to individual container pairs inside phase 3), phases 3 and 5 were deleted outright, and
  the paper now describes five phases rather than seven.
* **GumTree is v4.0.0-beta8**, not the beta4 earlier runs used; the beta4 tree no longer exists on
  disk. beta8 adds C++ and TSX generators and drops JSON, so GumTree's scored subset is not
  comparable across that boundary. See `research/data/comparison/PROVENANCE.md`.
* **RQ3 gained a change-shape census** (then Table 3), which was the "which shapes occur" half the
  section previously left to the ablation alone. **Superseded on 2026-08-22** - RQ3 was pivoted to
  ground-truth ambiguity and the census was cut from the paper (see the RQ1-RQ3 rule above). Its
  producer, macros and `make shapes-report` target are all still live and still describe the
  corpus: reparenting is 26.7% of *fixtures* but 58.5% of CodeDiff's *mismatches*, and 35.3% of
  that error sits in fixtures matching none of the censused shapes. Restoring the table is a
  `main.tex`-only edit.

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
