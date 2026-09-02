# Introductory paper

An ACM `sigconf`-format LaTeX paper about syntax-aware code diffing. **It is a research paper with
a tool contribution at the end, not a tool paper** - a distinction set deliberately on 2026-08-29
and easy to erode. Restructured on 2026-09-02 around four research question *groups*, stated in the
introduction and each answered in its own section, in order; the tool comes last. Structure:

1. **Introduction** (the four RQ groups, sub-numbered RQ1.1, RQ1.2, RQ2, RQ3.1-RQ3.3, RQ4.1,
   RQ4.2), 2. **Background** (the algorithms and tools the field is built on - now including
   Unix diff, git's four algorithms, BDiff and Neovim's diff mode)
3. **Empirical Dataset** - the repository lists, file/edit shape, how the fixture corpus was
   sampled and solved, and how the human mapping (multi-map groups included) and the human painting
   were authored. Methodology only; no RQ is answered here. Its "Shape of real-world file edits"
   subsection (Table 2) is our own census, added 2026-09-02 - see below.
4. **Viability** (RA1.1 expressibility: the N:M rate from the multi-map groups; RA1.2 cost: how
   often the human mapping is strictly costlier than a heuristic's under unit costs)
5. **Speed** (RA2) - what a whole-tree tree-edit-distance computation costs on real files
6. **Uniqueness** (RA3.1 mapping uniqueness, RA3.2 visualization uniqueness, RA3.3 the visible
   share of the AST) - the ground truth measured against itself, no diffing tool in the section
7. **Implementation** (RA4.1 accuracy, RA4.2 cost) - nine established tools against that ground
   truth. CodeDiff is absent by construction.
8. **CodeDiff** - the tool contribution: the pipeline, then its own node-level accuracy, its
   line-level rate on the same basis as Section 7's tools, speed and robustness.
9. **Related Work**, 10. **Conclusion**.

The tool comparison is written to be read as generous, not competitive: difftastic and diffsitter
optimize for human readability rather than ground-truth mapping fidelity, and GumTree's own
evaluation, on its own pipeline and corpus, is what motivated adopting its move-detection
heuristic in the first place - Section 7 and the Conclusion both frame the numbers as one
way of combining what each tool already does well, not as a claim that any of them are lesser.

**A framing decision made in the 2026-09-02 restructure, worth knowing before editing Sections
4 and 6:** the author's Dataset section reads a `MultiMapGroup` as an *N:M site* - a
correspondence the four operations cannot express - and RA1.1 counts groups on that reading. The
older text (and `data/quality/nm_instances.md`) read a group as a set of *interchangeable*
candidates and kept N:M as an unencoded existence result. The paper now reconciles the two in
one sentence in Section 3: a group records the N:M site, and the ground truth scores it by its
closest one-to-one approximations (any consistent `min(N,M)` pairing). RA3.1 then derives
"multiple optimal mappings" from the same sites plus a second, format-independent reading: cost
ties between the human mapping and the harness's own matcher's mapping at a *different* mapping
(`CostTieDifferent*` macros). Do not re-split them without touching both sections.

**The title still reads as a tool-paper title** ("CodeDiff: A Fast, Robust, Production-Ready,
Syntax-Aware Code Diffing Tool") and no longer matches the paper's shape. Left alone deliberately -
retitling is the author's call.

## Status

Compiles cleanly, 16 pages, builds with `latexmk -pdf -g main` (verified locally, `cm-super` +
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
  (`ambiguity_report.py` -> `variables_ambiguity.tex`, via `make ambiguity-report`), what reaches
  the reader (`rendering_report.py` -> `variables_rendering.tex`, via `make rendering-report`), and
  the change-shape census (`human_mapping_shapes_report.py` -> `variables_shapes.tex`, via `make
  shapes-report`) - the last currently generated but unreferenced, see the RQ rule below.
* **Authored** - no saved producer to read back from, so the value is transcribed, but into one
  version-controlled place with a comment naming the command that produced it. This covers the
  corpus/node-accuracy totals, the robustness run, and the design targets. The `Ablation*` block is
  still generated and now entirely unreferenced - see the 2026-08-29 note in the RQ rule below.

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

### Every RQ is answered without reference to CodeDiff

Set on 2026-08-22 for RQ1-RQ3, widened on 2026-08-29 to all six, and kept through the 2026-09-02
regrouping with one deliberate exception: RA1.2 and RA3.1's cost readings need *some* algorithm's
mapping to compare the human's cost against, and the harness's own (CodeDiff's) is the one on
disk. Section 4 names it in a footnote, tool-neutrally, for the same reason Section 5 names the
APTED implementation - so the "lower bound" caveat is checkable. The section numbers in the rest
of this note predate the regrouping (old Section 5 = Uniqueness/Viability material, old Section 6
= Implementation, old Section 7 = CodeDiff). It is a structural rule, not a
stylistic preference: an RQ answer describes the problem or the state of the art, so no RA box and
none of the argument supporting one mentions CodeDiff. Section 3 keeps one attribution - the RQ1
measurement names the APTED implementation it ran, because that is what makes its "lower bound, not
upper bound" caveat checkable - but states it tool-neutrally. **RQ2, RQ3 and RQ4 go further: their
whole section (5) names no diffing tool at all**, not even the four compared later, because they
are properties of code change rather than of any implementation. Consequences that are easy to undo
by accident:

* **The four-tool comparison excludes CodeDiff entirely.** Table 2 has four rows, and
  `benchmark_other_report.py::plot_accuracy` deliberately drops the `codediff` series (see its doc
  comment). Since 2026-08-29 the speed table (`tab:speed-vs-others`) excludes it too, because
  RQ6 makes tool cost an RA rather than a production-viability note; CodeDiff's own percentiles are
  prose in Section 7, quoting the same macros, so nothing can drift. `plot_runtime` still keeps
  CodeDiff in `fig:runtime`, which is why that figure sits in Section 7 rather than Section 6 - it
  cannot be regenerated without CodeDiff's series. The `\CodeDiffLineRate{}` family of macros is
  still generated and simply unused; that is intentional, so the number is one edit away if the
  framing changes back.
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
* **The ablation study was cut on 2026-08-29.** It is a measurement of *this pipeline*, which is
  a tool-paper result; it says nothing about the problem or the state of the art, and it was the
  last thing in the paper arguing from CodeDiff's internals. Removed together with its table, its
  Related Work and Conclusion citations, and the phase-5 sentence that quoted it. **Nothing about
  the study itself was retracted** - `ablation_study.sh`, the `Ablation*` macros and the
  `HeuristicConfig` doc comment are all untouched and still correct, so it is one edit from
  returning if a tool paper wants it. Its "unique type matching fires zero times" result is still
  the sharpest thing measured about this pipeline.
* **RQ4 (rendering) and RQ3 (visibility) were added on 2026-08-29**, both answered by
  `analysis/rendering_report.py` (`make rendering-report`). They are the reason Section 4 exists as
  a separate methodology section: the painted text ground truth needed describing before either
  could be stated. Two properties of that script matter. It scopes itself to
  `human_mapping_analysis.csv` exactly as `ambiguity_report.py` does, so its denominators match the
  rest of the paper. And **the painted subset is not a random sample** - painting is manual, and
  all 43 painted fixtures are from the `handmade` category, median 14 LOC against 219 for the rest
  of the corpus. Section 5 states that limit explicitly and argues it runs conservative; do not
  quote `\PaintingDualPct{}` as a corpus-wide rate.

Two properties of `ambiguity_report.py` are load-bearing and easy to break. It scopes itself to
the fixture set in `data/quality/human_mapping_analysis.csv`, so Section 5 keeps describing one
corpus state. And it reads each fixture's annotation era from the committed
`data/quality/ambiguity_eras.csv` rather than re-deriving it from `git log` on every run: the
multi-mapping facility postdates part of the corpus, so the headline rate depends on that
classification, and without the committed record one formatting sweep over `src/test/data/diffs/`
would move every pre-facility fixture into "revisited" and change the rate with no corpus change
at all. `--refresh-eras` re-derives deliberately.

### The comparison covers nine tools, and since 2026-09-02 the paper's tables show all of them

Added 2026-08-23: four git algorithm variants (`git_myers`, `git_minimal`, `git_patience`,
`git_histogram`) and BDiff (`bdiff`), all text-based, all covering the full corpus. They are wired
into `benchmark-timing`, `benchmark-accuracy`, both CSVs, the charts and the generated macros, so
every future re-benchmark includes them with no further work.

**Why CodeDiff's line rate flips sign between the two subsets** (0.795% on its own 493 fixtures,
1.265% on the 262 common ones): a pooled line rate weights a fixture by its length, and 5 long
fixtures hold 89% of CodeDiff's common-subset mismatched lines on 16% of its lines. Without them
it is 0.161% against git's 0.541%, and per fixture it never trails at all (221 perfect vs 123, 114
fixtures better vs 25 worse). `common_subset_concentration` in `paper_variables.py` derives the
`Common*` macros Section 8 uses to say this; its `COMMON_SUBSET_TOOLS` must stay in step with
`benchmark_other_report.py`'s `PAPER_MACRO_STEMS`, and it warns if the two disagree about
`\CommonFixtures`. An earlier draft of that paragraph blamed the subset's mainstream-language mix,
which the data does not support - the concentration is 5 named fixtures, three of them documented
RA1.1/RA1.2 cases and two open unreviewed gaps.

`main.tex` was changed on 2026-09-02 to add rows for them: `tab:accuracy-vs-others` and
`tab:speed-vs-others` now carry every series the macros define, grouped like the generated bucket
table, and `\CodeDiffLineRate{}`/`\CommonCodeDiffLineRate{}` are printed in Section 8 (both
subsets, with the sign flip between them stated rather than chosen away). `Shape*` and `Ablation*`
remain generated and unreferenced. What the run found:

* The four git variants and Unix `diff` agree with the human mapping to within 0.01 percentage
  points of each other (1.12-1.13%). `git_minimal` is bit-identical to `git_myers` on every
  fixture. Choice of line-diff algorithm does not move this metric, which closes the obvious
  reviewer objection that RA2's baseline used "only" Myers.
* Nugroho et al.'s "use --histogram for code changes" (EMSE 2020) does **not** transfer here:
  histogram is marginally the *worst* of the four against human ground truth. Their criterion is
  edit-script size and miner readability, not agreement with a human mapping - a different
  question, answered differently, which is the same "a published benefit is a property of the
  metric and pipeline it was measured in" point the ablation makes.
* BDiff (1.18%) does not beat plain git on this metric either, despite being block-aware.

Two measurement notes worth keeping. BDiff's per-process time is ~97% Python import overhead, so
it is reported cold *and* warm exactly like GumTree's JVM (315.6 ms against 7.8 ms median). And
per-tool *mean* runtimes in this corpus are distorted by run order - whichever tool goes first per
fixture absorbs cold-cache cost, which is why `unix_diff` and `git_myers` show ~18 ms means
against ~2.5 ms medians. Quote percentiles, not means; the paper's speed table already does.

### The edit-shape census (Section 3.2, Table 2)

`analysis/edit_shape_stats.py` (`make edit-shape MODE=small`) walks the most recent
`EDIT_SHAPE_COMMITS` (50, matching `\CorpusCloneDepth`) non-merge commits of each cloned
repository and reports how big a real-world edit is. It replaced a "TODO: Add our own metrics
here" that had stood in Section 3.2 next to the Arafat and Riehle citation.

Four decisions in it are load-bearing, each made after measuring the alternative:

* **The commit cap is not a speed knob.** These clones are shallow but *not uniformly so*:
  `torvalds-linux.git` alone holds 1.29M of the corpus's 2.31M reachable commits. An uncapped walk
  produced 19.7M file edits with a median of 65 changed lines per file - which is the Linux
  kernel's median, not the corpus's. Capped at 50 per repository the median is 2.
* **Creations and deletions are excluded** from every distribution (they are 9.5% of code-file
  edits). A file with only one version presents nothing to map, and including them roughly doubled
  the churn median.
* **Churn is derived, not joined.** `lines_after` comes from one `git cat-file --batch` per
  repository, and `lines_before = lines_after - added + removed` follows exactly, so all 47,980
  modifications get a fraction. An earlier version joined `stats.sqlite`'s `commits` table and
  covered 6,000 of 19.7M edits, 0.03%, selected differently - not a usable denominator.
* **`stats.sqlite` cannot answer this question at all.** Its `lines_added`, `lines_removed`,
  `lines_changed` and three `nodes_*` columns are hardcoded to zero in `commit_stats.rs` ("the
  actual diff processing will be implemented later"), and its `nodes_before`/`nodes_after` are
  `root_node().child_count()`, i.e. direct children rather than subtree size (max 3,495 over the
  whole corpus). Do not read any of those seven columns.

Consequences worth keeping: the census reports **no AST-node churn** - that needs both sides
parsed, which is exactly what `commit_stats.rs` would do if the columns above were implemented -
so Section 3.2 labels its line-level fraction as a proxy and Section 8's phase 1 leans on it as
one. The artifact in `data/corpus_stats/edit_shape.csv` is per-language rows only; the per-edit
population is far too large to commit.

### Known-stale numbers

**The empirical-study numbers (Section 3, Table 1, corpus size) currently reflect the 100-repository
`small` sample, not the paper's eventual full 7,423-repository corpus.** This is deliberate, not a
bug: computing the full corpus's numbers for real means running `file_stats` (tree-sitter parsing
+ AST-node counting) over ~7,445 already-cloned repositories, which a timing probe against the
100-repo sample put at roughly 24-25 hours and ~400GB of database (measured 2026-07-31: 100 repos
took 19m50s, 1.35M files, 1,133 files/sec). Swapping in the real full-corpus numbers, once that run
finishes, is `make file-stats MODE=full` (slow, run once) followed by `make
introductory-paper-empirical MODE=full` (fast, re-renders and rebuilds) - no hand-editing required.

**Every ground-truth number was refreshed on 2026-08-20** against the current 468-fixture corpus
(469 fixture directories, 468 of them carrying a `human_mapping.json`), replacing numbers measured
on 98 fixtures. Refreshed together, deliberately: AST-node accuracy, the ablation study, the
per-tool line-level comparison, the speed percentiles, and the robustness run were all measured
against the same corpus, so refreshing one block alone would leave the paper internally
inconsistent. Keep that property on the next refresh. **Those numbers are now spread across
Sections 5, 6 and 7** after the 2026-08-29 restructure - the constraint is unchanged and now spans
three sections rather than one. The rendering block (Section 5's RQ3 and RQ4) was added on
2026-08-29 against the same corpus state, `rendering_report.py` scoping itself to the same CSV.

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
