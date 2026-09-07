# Provenance

Everything in this directory, plus `../../plots/variables_empirical.tex`,
`../../plots/variables_edits.tex` and the three corpus figures
(`../../plots/{tips,language_distribution,ast_nodes_bytes_correlation}.png`), comes from one
measurement of **The Full List** on 2026-09-07. It replaces the Curated-100 measurement that every
empirical number in `papers/introductory-paper` was previously based on.

The superseded Curated values are kept alongside under a `_curated` suffix, but **only where their
provenance could be established from git**: `variables_empirical_curated.tex`,
`variables_edits_curated.tex`, `code_percentiles_curated.csv`,
`top_node_kinds_by_language_curated.md` and `tips_curated.png` are all copies of committed files.

There is deliberately no `language_distribution_curated.png` or
`ast_nodes_bytes_correlation_curated.png`. Both figures were untracked working-tree leftovers
before this run, not committed artifacts, so which corpus produced them cannot be established and
labelling them "curated" would have asserted something untrue. Regenerate them from
`/var/tmp/research/small/stats.sqlite` if a genuine Curated pair is wanted.

## The run

| | |
|---|---|
| Date | 2026-09-07 |
| Corpus | `/var/tmp/research/full/repositories`, `DEPTH=50` |
| List | `list_of_repositories.csv`, 7,491 data rows |
| Repositories cloned and measured | **7,444** |
| Failed to fetch | 135 (see below) |
| `files` rows in `stats.sqlite` | **7,045,754** |
| Distinct projects in `stats.sqlite` | **7,444** |
| Database size | 27 GB |
| `EDIT_SHAPE_COMMITS` | 50 |
| Machine | Intel Xeon E3-1275 v5, 4c/8t, 64 GB RAM, 4x16 TB HDD in RAID 5 (see the paper's `MACHINE` block in `analysis/paper_variables.py`) |

Wall clock, in the order run:

| Step | Time |
|---|---|
| `make fetch MODE=full DEPTH=50` | 2h38m |
| `file_stats` over the whole root (aborted, see below) | 0h29m |
| `file_stats` per repository (the run that counts) | 2h38m |
| `analysis/file_stats.py` (report, figures, percentiles) | 0h30m |
| `analysis/edit_shape_stats.py` (discarded, see below) | 2h08m |
| `analysis/edit_shape_stats.py` (the run that counts) | 2h43m |

## `\NumRepos` is 7,444, and it is not 7,491 minus 135

The failure count overstates what is missing. Of the 135 entries that failed to fetch:

* **89** still had a working clone from an earlier fetch, so they were parsed and counted anyway.
  A failed `git fetch` does not remove a checkout.
* **46** had no clone on disk.
* **9** of the 135 are malformed rows in the source list, unclonable by construction: the Gentoo
  package list emitted symlink descriptions as project names, so `list_of_repositories.csv:101`
  reads `akallabeth.gpg -> openpgp-keys-akallabeth-20240521.asc` and the derived URL is
  `https://github.com/akallabeth.gpg -> .../...`.

`7,491 - 135 = 7,356` would therefore have understated the corpus by 88 repositories. The number to
quote is the one `analysis/file_stats.py` derives from the database itself
(`project.n_unique()`), which is 7,444 and matches the directory count on disk exactly.

A related defect in the same list: nine directory names carry a `.git` suffix
(`d3-d3.git`, `axios-axios.git`, ...) because the CSV lists the clone URL with the suffix. Those
are the nine repositories `edit_shape_stats.py` reports `git log failed` for.

## One file is excluded from the parse

`file_stats` over the whole corpus root aborts:

```
tree-sitter-0.25.10/src/parser.c:415: ts_parser__external_scanner_serialize:
Assertion `length <= 1024' failed.
```

The file responsible is
`FasterXML-jackson-dataformats-text/yaml/src/test/resources/data/fuzz-65918.yaml`: 9,478 bytes on a
single line, 4,349 `-` and 5,127 spaces and almost nothing else, i.e. roughly 4,300 nested YAML
block sequences. It is an OSS-Fuzz regression artifact, not code. `tree-sitter-yaml`'s external
scanner serializes its indentation stack into a fixed 1,024-byte buffer and that many levels
overruns it. This is a C `abort()`, so - like the stack overflows `file_stats.rs:109` already
guards against - nothing in-process can catch it.

The corpus was therefore parsed **one repository per subprocess**, which bounds an abort to the
repository that caused it. That is the only difference from a single whole-root run: same binary,
same corpus, same schema, same rows (`files.path` is UNIQUE and re-running upserts, see
`file_stats.rs:308`). `FasterXML-jackson-dataformats-text` was then re-parsed with that one file
held out, recovering its other 1,773 files.

**One file of 7,045,754 is missing from these numbers.** No other file was skipped.

## Edit shape: shallow-boundary commits are excluded, and the first run did not exclude them

`analysis/edit_shape_stats.py` gained `shallow_boundary_commits()` in the same commit as this file.
It reads `.git/shallow` and drops those commits' `--numstat` rows.

This is not a refinement, it is a correctness fix. A shallow clone tells git its graft commits have
no parents, so `git log --numstat` reports each of them as **creating every file in its tree**.
Walking 50 commits of a corpus cloned at depth 50 hits that boundary in almost every repository.
Measured over a 60-repository sample: **90.9% of all numstat rows in the 50-commit walk came from
commits listed in `.git/shallow`**.

The first run of this measurement did not exclude them and produced:

| Macro | Contaminated | Corrected | Curated (depth 1000) |
|---|---|---|---|
| `\EditsModifiedSharePct` | 7.2 | **75.2** | 90.5 |
| `\EditsCodeFileEdits` | 6,016,305 | **578,783** | 53,017 |
| `\EditsCodeSharePct` | 46.6 | **31.5** | 64.3 |

Every other macro in the fragment is identical between the two runs, because `_classify` already
excluded creations and deletions from every distribution - only the three ratios above read the
raw edit counts. The Curated measurement was never affected: that corpus is cloned at depth 1000
but walked 50 commits deep, so it never reaches its own graft.

The residual gap between 75.2% and the Curated 90.5% is real, not an artifact: the Gentoo-derived
long tail holds many small and young repositories, where a larger share of edits genuinely are file
creations.

## What is not here

`make measure-commit-stats MODE=full` was deliberately not run. No paper macro reads the `commits`
table, and `stats.sqlite`'s `lines_added`/`lines_removed`/`nodes_*` columns are hardcoded to zero
by `commit_stats.rs`.
