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

## The whole distributions, 2026-09-18

`code_file_size_distribution.csv` and `edit_shape_distribution.csv` are the two populations
above in full, as `metric,value,count` rows, added so the introductory paper can draw them as
cumulative curves (its corpus-shape figure, `analysis/distributions_report.py`) instead of
quoting four percentiles of each. Neither is a new measurement:

* The file-size file is a re-read of the same `stats.sqlite` the 2026-09-07 run left behind,
  written by `analysis/file_stats.py`'s new `export_size_distribution` during
  `make file-stats-report MODE=full` (2026-09-18, about 30 minutes). Same code-only filter as
  `code_percentiles.csv`; the empty and unparseable files are in it, as they are in Table 1's
  percentiles.
* The edit-size file needed the corpus walk re-run, because the 2026-09-07 walk kept only
  aggregates. `analysis/edit_shape_stats.py --repositories /var/tmp/research/full/repositories
  --max-commits 50`, started 15:50 and finished 17:47 on 2026-09-18 (1h57m; the machine was
  otherwise busy with a report and a smaller walk for the first half hour). Its `edit_shape.csv`
  and `variables_edits.tex` were byte-identical to the committed ones, so the walk is the same
  population and the distribution file is exactly the one behind the committed percentiles. 11
  repositories reported `git log failed`, against 9 on 2026-09-07; two more clones have lost
  their HEAD since, which does not move any number at the precision the paper prints.

## What is not here

`make measure-commit-stats MODE=full` was deliberately not run. No paper macro reads the `commits`
table, and `stats.sqlite`'s `lines_added`/`lines_removed`/`nodes_*` columns are hardcoded to zero
by `commit_stats.rs`.

## The file-type classifier changed after this run

`src/code/tip.rs` was widened on 2026-09-13 (many more extensions and file names, measured with
the new `reclassify_tips` binary), so `tips.png` and the `Unknown` share it shows reflect the
*old* tables: on the dev machine's Full List snapshot the same change moves Unknown from 29.3% to
6.6%, almost all of it into Data and Code. To refresh the figure without re-walking the corpus,
run `make reclassify-tips MODE=full RECLASSIFY_FLAGS=--write` and then `make file-stats-report`
against this run's `stats.sqlite`, and record it here. Rows reclassified that way carry no
size/AST numbers (they were never read), so `code_percentiles.csv` is unaffected by them until the
corpus is re-walked. **Superseded:** the corpus was re-walked on 2026-09-24, see the last section
of this file; every figure and macro now reflects the widened tables.

## The files above 1 MiB, 2026-09-20

Until 2026-09-19 `stats::expand_from_code` did not parse a file over 1 MiB (`too_large_to_parse`,
no node count), a guard from the initial commit with no measured reason. The 4,014 code files above
that size (19.9 GB of source, the largest 101 MB) were therefore counted in bytes and lines but
absent from every AST-node figure - including the 905,004-node maximum the paper's Robust target
was set from, which the whole-corpus robustness run then exceeded by a factor of eight on pairs
it completed. The cap was removed and those files measured in place:

| | |
|---|---|
| Command | `file_stats --path /var/tmp/research/full/repositories --db /var/tmp/research/full/stats.sqlite --min-bytes 1048576`, as a systemd unit capped at 48 GB |
| `--min-bytes` | new: re-processes one size class and upserts by path, so the other seven million rows are untouched |
| Wall clock | 2026-09-20 00:27 end; 1h12m CPU across 7 workers, 22 GB memory peak |
| Outcome | 3,474 parsed, 9 gave up at the 60 s parse budget, 531 flagged generated (skipped, as always); `too_large_to_parse` is now 0 everywhere and stays in the schema |
| New maximum | 23,584,040 nodes, `MycroftAI-mimic1/lang/vid_gb_ap/vid_gb_ap_cg_12_params.c` (83 MB of voice-model parameters) |

Two harness faults surfaced and were fixed on the way. The first attempt aborted on a stack
overflow after 774 files: `count_nodes` and `visit_for_kind_stats` recursed once per tree level,
and a file nested thousands deep beat even the 256 MB worker stack; both walks are iterative now,
with a 50,000-level test. The second attempt then had all seven workers stuck for over an hour on
five 2-4 MB `.h` files holding nothing but a comma-separated byte array to be `#include`d into an
initializer - not C at top level, so tree-sitter stays in error recovery for the whole file, its
one super-linear path. A 60-second per-file parse budget (tree-sitter's progress callback) now
records such a file as `failed_to_parse`; the 9 above are those.

What moved in `variables_empirical.tex`: `\AstMax` 905,004 -> 23,584,040, `\AstPNinetyNine`
23,948 -> 25,674, and `\CorrelationR` 0.8986 -> 0.4702. The last is Pearson over a population
that now has a heavy tail: the files above 1 MiB are generated data whose bytes per node run from
three to thirteen, and Pearson follows its largest points. Within the 99th percentile of both size
measures - the population the bytes/5 fit is drawn from - r is 0.9133 (`\CorrelationRTrimmed`,
new), and the median bytes per node is 4.8 either way; the paper reports both and says why.

Note what a zero node count means in this database, since 311,112 code files (8.0%) carry one:
177,991 are flagged `automatically_generated` from a header comment and never parsed (a rule older
than this run), 128,288 are in a language the classifier knows but codediff has no grammar for,
4,824 are empty, 9 gave up. The largest files in the corpus are generated too, but carry no such
comment and so are parsed; that is why the corpus-shape figure's nodes curve starts flat and its
maximum is a data table.

## Re-measured with the 2026-09-13 classifier, 2026-09-24

The 2026-09-24 paper review asked why Figure 2 still showed 30% of files as Unknown after the
classifier tables in `src/code/tip.rs` had been expanded on 2026-09-13. Because the `tip` column
is what `file_stats` stores at measurement time, and the 2026-09-20 pass re-processed only the
files above 1 MiB, the answer was that nothing had re-measured the rest. Writing the new
classification back by path alone (`make reclassify-tips RECLASSIFY_FLAGS=--write`) was
considered and rejected: a file moved out of Unknown that way was never read, so 805,630 rows
would have joined the code-file distributions with no bytes, lines or node count. The corpus was
walked again instead.

| | |
|---|---|
| Date | 2026-09-24, 18:01 to 20:54 for the walk, then the report |
| Command | `file_stats --path <repository> --db /var/tmp/research/full/stats.sqlite`, one process per repository over `/var/tmp/research/full/repositories/*/`, in alphabetical order, as a systemd user unit capped at 48 GB (`codediff-fullwalk-20260924`) |
| Repositories | 7,444, 0 non-zero exits |
| Wall clock | 2h53m for the walk; `analysis/file_stats.py` about 20 minutes, run twice (once by `make file-stats-report`, once as `make introductory-paper-empirical`'s prerequisite) |
| Held out | `FasterXML-jackson-dataformats-text/yaml/src/test/resources/data/fuzz-65918.yaml`, renamed for the duration and restored, for the reason under "One file is excluded from the parse" above |
| Binary | `target/release/file_stats` at commit `dc063239`, tree-sitter 0.25.10, the 60-second per-file parse budget of the 2026-09-20 pass in place |

Rows are upserted by path, so the run replaced every row's measurements in place and the table
still holds 7,045,754 files. One row was added by mistake and deleted afterwards: the held-out
file was walked under its temporary `.held-out` name and inserted as an Unknown file, so
`files.path LIKE '%fuzz-65918.yaml.held-out'` was deleted and the report re-run before anything
was committed. The original `fuzz-65918.yaml` row still carries its 2026-09-07 measurement.

What moved:

| category | 2026-09-07 | | 2026-09-24 | |
|---|---|---|---|---|
| Code | 3,890,994 | 55.2% | 4,688,419 | 66.5% |
| Data | 911,023 | 12.9% | 1,593,317 | 22.6% |
| Unknown | 2,112,884 | 30.0% | 434,318 | 6.2% |
| Configuration | 84,973 | 1.2% | 176,475 | 2.5% |
| Documentation | 45,880 | 0.7% | 153,225 | 2.2% |

The numbers agree with the `reclassify_tips --db` dry run of the same day to the row, which is
the check that the re-walk classified the same way the reclassification would have.

The code-file distributions moved because their population grew by a fifth, and the newcomers
run small: `\LocPFifty` 62 -> 43, `\LocPNinety` 448 -> 375, `\LocPNinetyNine` 2,914 -> 2,511;
`\AstPFifty` 335 -> 206, `\AstPNinety` 3,482 -> 2,876, `\AstPNinetyNine` 25,674 -> 22,005;
`\BytesPFifty` 2,353 -> 2,238. The maxima are unchanged. `\CorrelationR` is unchanged at 0.4702
and `\CorrelationRTrimmed` 0.9133 -> 0.9085; the trimmed fit is
`nodes = 0.207 * bytes + 44`, so the paper's bytes/5 rule of thumb stands.

One thing to know when reading the node percentiles: **820,078 of the 4,688,419 code files
(17.5%) have no tree-sitter grammar** (`language` is null), against 128,288 before, because
the expanded tables classify Perl, Makefile, CMake, Lisp, Fortran, assembly and similar
extensions as Code. Those files are counted with a node count of zero, as the 2026-09-20
section above explains for the smaller set, so the nodes curve's flat start in the paper's
corpus-shape figure is now a fifth of the population rather than a twelfth, and `\AstPFifty` is
pulled down by them more than `\LocPFifty` is. The convention was kept rather than changed
here; excluding no-grammar files from the node distribution alone would make the three
top-row curves of that figure describe three different populations.
