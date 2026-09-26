# Introductory paper

"CodeDiff: A Fast, Robust, Syntax-Aware Code Diffing Tool", an ACM `sigconf` LaTeX paper. It is a
research paper with a tool contribution at the end, not a tool paper: the introduction states four
groups of research questions, each is answered in its own section, and the tool comes last.

1 Introduction, 2 Background, 3 Empirical Dataset (methodology only, no RQ answered),
4 Viability (RQ1.1, RQ1.2), 5 Speed (RQ2), 6 Uniqueness (RQ3.1-RQ3.3), 7 State of the Art
(RQ4.1, RQ4.2: eight established tools in eleven configurations), 8 CodeDiff, 9 Threats to
Validity, 10 Related Work, 11 Conclusions. The ACM conference, DOI, ISBN and rights fields are
placeholders until a venue is chosen.

## Rules that are easy to break

* **No RQ answer names CodeDiff.** An RA box and its supporting argument describe the problem or
  the state of the art. RA1.2 and RA3.1 compare the human mapping's cost with the harness's own
  matcher; the text describes it tool-neutrally. CodeDiff's own results are in Section 8.
* **RA1.1 and RA3.1 read the same multi-map groups two ways**: as an N:M site the four edit
  operations cannot express (RA1.1), and as a set of equally correct one-to-one pairings (RA3.1).
  RA3.1 adds a second reading that needs no annotation, cost ties at a different mapping
  (`CostTieDifferent*`). Change the two sections together.
* **Every measured number is a macro.** `main.tex` has no bare numeric literal from a
  measurement. Macros live in `figures/variables.tex`, written by
  `research/analysis/paper_variables.py`, which documents each block's source. Generated blocks
  are read back from measurement files; authored values (`CORPUS`, `TARGETS`, `MACHINE`, ...) are
  edited in `paper_variables.py`, never in `variables.tex`, which every build overwrites. A macro
  with no value prints a bold `??`: `-interaction=nonstopmode` does not fail on an undefined macro.
  If `plots/variables_empirical.tex` is missing, the assembler carries the previous empirical
  values forward rather than printing `??`, because the slow `measure-file-stats` run is not a
  prerequisite of the paper targets.
* **`figures/` holds symlinks into `research/plots/`**, not copies.
* **Refresh corpus-dependent blocks together** - ground-truth totals, the ambiguity, rendering and
  shape reports, the tool comparison, the fixture robustness run and the AST-diff oracle runs. The
  order: `benchmark_optimal_solutions --csv`, then `analyze_human_mappings --csv` (see
  `data/quality/PROVENANCE.md`) and the `CORPUS` block; `make ambiguity-report`,
  `rendering-report` and `shapes-report`; `make measure-tools-accuracy` before `make
  measure-tools-timing`; `make measure-robustness-fixtures`; `make measure-astdiff-oracle` and
  `measure-astdiff-oracle-human`. Afterwards, re-read the prose's ordering claims (best tool,
  fastest tool): macros cannot check them. `benchmark_other_report.py` warns when srcDiff and
  difftastic are no longer the two established tools with the highest Perfect share.
* **Four datasets**, `_common.PAPER_DATASETS`: Curated (`small`), Full, Stratified, Defects4J.
  `handmade` fixtures are regression tests, not a sample, and stay out of the paper. Only fixtures
  with a `human_mapping.json` are scored, and which Defects4J units are solved is how far
  annotation has reached, not a draw. The painted fixtures are not a random sample either (see
  `analysis/rendering_report.py`). The text is written as if every dataset were finished;
  unfinished passes are marked only in footnote `fn:in-progress` and the asterisked rows of the
  expressibility table. `REVIEWED_LISTS` in `ambiguity_report.summarize` names the datasets whose
  ambiguity pass is complete.
* **`Defects4J` is not a valid macro name**; macros spell it `DefectsFourJ`
  (`benchmark_other_report.macro_stem`, `ambiguity_report.PER_LIST_DATASETS`).
* **"RQ1" in file and macro names is the paper's RQ2** (`data/rq1/`, `\RqOne*`, the `RQ1_GROUP_*`
  lists in `research/Makefile`); see `analysis/apted_only_report.py`'s docstring.
* **Lists that must stay in step**: `paper_variables.COMMON_SUBSET_TOOLS` with
  `benchmark_other_report.PAPER_MACRO_STEMS`, and the dataset list in
  `src/bin/benchmark_diff_pairs.rs` with `_common.PAPER_DATASETS`.
* **Generated tables use `main.tex`'s series names** (`LATEX_NAMES`; `DISPLAY_NAMES` is for
  matplotlib only, and `_escape_tex` is not applied to `LATEX_NAMES`). The tool count appears in
  the prose, the table captions and the captions `benchmark_other_report.py` generates; change them
  together.
* **Figures are vector and greyscale-safe**: each plot script writes a `.pdf` beside its `.png`,
  `main.tex` includes figures without an extension, and bars carry hatching as well as hue.
  `plots/tips.png` needs the corpus database `stats.sqlite`, which is not committed: `make
  file-stats-report MODE=full` re-reads it on the measuring machine.
* **Some generated macros are unused on purpose** - `Shape*`, `Ablation*`
  (`scripts/ablation_study.sh`, an older corpus state), `OracleHumanCodeDiff*` - so a cut result
  can return with a `main.tex` edit only.

## Reviews

`REVIEW-<date>.md` records each of the author's annotated reviews of a PDF: every mark and what was
done about it.

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

* `make introductory-paper` - fast (seconds). Regenerates `plots/variables.tex` from whatever is
  on disk and rebuilds the PDF. It does not re-render the tool-comparison charts and tables; `make
  timing-report` does that, from `research/data/comparison/`.
* `make introductory-paper-empirical MODE=<tiny|small|full>` - re-renders Table 1 and friends from
  whatever `MODE`'s `stats.sqlite` already has, and rebuilds the PDF. Seconds on `tiny`, about half
  an hour on `full`. Does *not*
  run `file_stats` itself - that's `make measure-file-stats MODE=<mode>`, and it's the slow one
  (hours on the full corpus; see `data/corpus_stats/PROVENANCE.md`).
* `make paper-variables` - fast (instant). Just regenerates `plots/variables.tex` from whatever
  is already on disk, without rebuilding the PDF.
