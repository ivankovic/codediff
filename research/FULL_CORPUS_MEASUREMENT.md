# Full-corpus measurement run (for the server agent)

**Goal:** produce the introductory paper's empirical numbers over **The Full List**
(`list_of_repositories.csv`, 7,490 repositories) instead of the Curated 100
(`list_of_repositories_100.csv`), which is what every empirical number in the paper is
currently measured on.

**Deliverable: measurement artifacts on a branch.** Do *not* edit `main.tex`, do *not*
run `make paper-variables`, do *not* rebuild the PDF. Choosing which numbers the prose
should quote is an authoring decision that stays with the author; see
[Why this matters](#why-this-matters) and [Do not overwrite the curated
numbers](#do-not-overwrite-the-curated-numbers).

All commands run from `research/` in a checkout of this repository.

---

## Why this matters

`research/papers/introductory-paper/main.tex` §3 (Empirical Dataset) describes two
lists: the Curated list (100 repositories, hardcoded in the prose) and

> "The Full List - `\NumRepos{}` repositories, created by expanding the Curated list with
> all repositories present in the Gentoo Linux distribution's package list"

but `\NumRepos` is currently **100** — the Curated count. Every other empirical macro
(`\NumFiles` 1,347,490, `\NumLanguages` 30, `\CorrelationR` 0.8794, and all of Table 1's
byte/LOC/AST-node percentiles) is likewise a Curated-only measurement. The paper also
carries two explicit TODOs at `main.tex:320-321`, on the edit-shape section:

```
TODO: Run for the full dataset as well
TODO: Present two datasets side by side
```

This run supplies the missing half.

---

## 0. Preconditions

* Disk. Anchors, not estimates: the Curated corpus is **100 repos at `DEPTH=1000` = 53 GB**
  and 1.35 M file rows; an earlier full-list run recorded **6.17 M file rows**. The full
  list is 74x the repository count at 1/20th the depth. **Check free space on `/var/tmp`
  before starting** and report what you have; stop and say so if it looks marginal rather
  than filling the disk.
* `uv` and a Rust toolchain; `make -C .. build` must succeed (it builds with
  `--features stats`).
* Everything lives under `/var/tmp/research/full/` (`REPOSITORIES_DIR` =
  `/var/tmp/research/full/repositories/`).

## 1. Delete the stale database first

`/var/tmp/research/full/stats.sqlite` may already exist (~1.7 GB, 6.17 M `files` rows).
**It is an old schema** — no `node_kind_counts`, no `node_kind_subtree_size_histogram` —
and `file_stats` creates tables with `CREATE TABLE IF NOT EXISTS`, so it will *not*
migrate it; `analysis/file_stats.py` then dies at `load_node_kind_counts` with
`RuntimeError: no such table: node_kind_counts`, and because `write_paper_variables` runs
*after* that call, no fragment is written at all.

```sh
make clean-db MODE=full        # rm -rf /var/tmp/research/full/stats.sqlite
```

Do **not** run `make clean MODE=full` — that also deletes the clones.

## 2. Fetch the corpus

```sh
make fetch MODE=full DEPTH=50
```

`DEPTH=50` is deliberate: `\CorpusCloneDepth{50}` in the paper records that the full
corpus was fetched at depth 50. Note that `git fetch --depth=N` *shortens* an existing
shallow clone as well as deepening it, so do not raise or lower this on a re-run.

`sampling/dataset.sh` clones **sequentially** and is **resumable** — an existing
directory gets a `git fetch --depth=N` instead of a fresh clone, so an interrupted fetch
can simply be restarted with the same command. This will take hours.

Failures are appended to **`/var/tmp/research/full/repositories/failed`**. With 7,391 of
the 7,490 entries coming from the Gentoo package list, a nontrivial number of dead URLs is
expected. **Report the contents (or at least the line count) of that file.** `\NumRepos`
must be the number of repositories actually *cloned and measured*, not the number of rows
in the CSV — this is the single number most likely to end up quietly wrong.

## 3. Preserve the curated fragments before measuring

Both generators write **fixed filenames with fixed macro names**, so a full-mode run
overwrites the Curated numbers in place and `paper_variables.py` silently picks up
whichever ran last. Since the paper wants both side by side, snapshot the curated values
first:

```sh
cp plots/variables_edits.tex plots/variables_edits_curated.tex
```

For the empirical block there is no committed `plots/variables_empirical.tex` — the
Curated values live inside the merged `plots/variables.tex` (the block under the comment
`% --- Empirical study: corpus size, per-file size percentiles, bytes-AST correlation.`,
`\NumRepos` … `\AstMax`). Copy that block out by hand:

```sh
sed -n '/^% --- Empirical study/,/^$/p' plots/variables.tex > plots/variables_empirical_curated.tex
```

Then rename its macros with a `Curated` infix (`\NumReposCurated`, `\NumFilesCurated`, …)
so the two fragments cannot collide. Leave the full-mode output at the default filenames
and default macro names.

Also copy aside the plots you are about to overwrite:
`plots/tips.png`, `plots/language_distribution.png`,
`plots/ast_nodes_bytes_correlation.png` and `data/corpus_stats/code_percentiles.csv`,
`data/corpus_stats/top_node_kinds_by_language.md`.

## 4. The two measurements

### 4a. File statistics — the §3 corpus block and Table 1

```sh
make measure-file-stats MODE=full
```

Runs `file_stats --path $REPOSITORIES_DIR --db $RESEARCH_DIR/stats.sqlite` (parses every
file in every clone; this is the long one) and then `file-stats-report`
(`analysis/file_stats.py`).

Writes:

| Artifact | Feeds |
| --- | --- |
| `plots/variables_empirical.tex` | `\NumRepos`, `\NumFiles`, `\NumFilesMillions`, `\NumLanguages`, `\CorrelationR`, and Table 1's `\Bytes*`/`\Loc*`/`\Ast*` percentiles |
| `data/corpus_stats/code_percentiles.csv` | Table 1's source of record (`data/README.md`) |
| `data/corpus_stats/top_node_kinds_by_language.md` | per-language node-kind census |
| `plots/tips.png`, `plots/language_distribution.png`, `plots/ast_nodes_bytes_correlation.png` | figures |

**Report alongside it:** the `files` row count and the distinct repository count in
`stats.sqlite`, so the macro values can be sanity-checked against the DB.

### 4b. Edit shape — the §3.x TODO

```sh
make measure-edit-shape MODE=full EDIT_SHAPE_COMMITS=50
```

Reads the clones directly with `git log --numstat` (no build, no parse), writes
`plots/variables_edits.tex` (`\EditsRepositories` … `\EditsChurnUnderTwentyPct`).

`EDIT_SHAPE_COMMITS=50` is **not** a speed knob and must not be raised: these clones are
shallow but not uniformly so, and an uncapped walk is dominated by `torvalds-linux.git`
(1.29 M of the Curated corpus's 2.31 M reachable commits), which would report one
repository's edit habits as the corpus's. It matches the clone depth from step 2.

### Not needed

`make measure-commit-stats MODE=full` — **do not run it.** No paper macro reads the
`commits` table; `variables_edits.tex` derives churn from `git log --numstat`, and
`stats.sqlite`'s `lines_added`/`lines_removed`/`nodes_*` columns are hardcoded to zero by
`commit_stats.rs`.

Nothing else in `research/Makefile` is in scope: the sampling, benchmark, timing,
ambiguity, rendering and shapes targets all measure the fixture corpus, not the repository
list, and are unaffected by MODE.

## 5. Hand back

Commit on a branch (e.g. `full-corpus-measurement`) and push:

* `research/plots/variables_empirical.tex` (full)
* `research/plots/variables_edits.tex` (full)
* `research/plots/variables_empirical_curated.tex`, `research/plots/variables_edits_curated.tex`
* the regenerated PNGs and `research/data/corpus_stats/*` (plus the curated copies)
* a short `PROVENANCE`-style note recording: the date, `DEPTH=50`,
  `EDIT_SHAPE_COMMITS=50`, the number of repositories in the list, the number
  successfully cloned, the number that failed, the `files` row count, and the wall-clock
  of each step.

Do not touch `plots/variables.tex` (it is generated), `main.tex`, or the PDF.

## Open authoring question — flag it, do not resolve it

`main.tex:261-262` reads "we built a dataset of `\NumFiles{}` code files in
`\NumLanguages{}` languages" *before* the two lists are introduced, so it scans as
describing the whole dataset while the values are Curated-only. Whether the full-list
numbers should replace those macros, or become a second set presented beside them, is the
author's call. Deliver both sets of artifacts and say in the branch description that this
is unresolved.
