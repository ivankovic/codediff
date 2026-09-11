# Provenance

`benchmark_other.csv` is measured against the ground-truth fixtures in `src/test/data/diffs/`
(same fixture set as `../quality/`, not the sampled corpus), by `benchmark_other` with external
tool binaries supplied via GUMTREE_BIN / DIFFT_BIN / DIFFSITTER_BIN / BDIFF_PYTHON. Rows are only
comparable within one run: tool versions and machine are not recorded per row, so refresh the
whole file, never append to it.

## Text-based tools (added 2026-08-23)

Nine tools are now scored, not four. Alongside Unix `diff` there are four git variants and BDiff:

* **`git_myers` / `git_minimal` / `git_patience` / `git_histogram`** - one engine (libxdiff)
  reached through `git diff --diff-algorithm=`, so they share a single labeller. Motivated by
  Nugroho, Hata and Matsumoto, "How different are different diff algorithms in Git? Use
  --histogram for code changes" (EMSE 2020), whose claim this corpus can test directly.
* **`bdiff`** - BDiff, block-aware text-based differencing (arXiv 2510.21094), cloned from
  <https://github.com/BDiff/BDiff> into `/var/tmp/bdiff-install` by `make install-bdiff`.

* **`nvim_diff`** - Neovim's own diff mode, driven headless by `assets/nvim_diff_driver.lua` and
  read back through `diff_hlID(lnum, col)`, which is the only public way to get at it: Neovim's
  diff result is window state, written to no stream. Always run with `-u NONE`, because `diffopt`
  is user-configurable and controls both the algorithm (`algorithm:histogram`) and the within-line
  alignment (`linematch:N`) - loading a user config would make this a measurement of that config.
  What is scored is Neovim's shipped defaults.

  It was briefly left out on the grounds that its line pass is libxdiff, so its line set matched
  `git_myers` on 38 of a 40-fixture sample. That was the wrong call: a 2-in-40 divergence is the
  same order as `git_myers` against `unix_diff`, which get separate rows, and "redundant at the
  granularity we happen to measure" is a claim about the metric rather than about the tool.
  Measured over the full corpus it differs from `git_myers` on 13 of 486 fixtures - better on 6,
  worse on 7, pooled rate 1.125% against 1.123%. Indistinguishable, but now by measurement.

**Two traps, both of which produce a silently wrong number rather than an error.**

1. **BDiff shells out to `git diff --no-index`**, so it inherits the user's git configuration.
   This project's own README recommends setting `diff.external=codediff`; with that set, git emits
   codediff's output, BDiff finds no `@@` headers, and `bdiff.bdiff()` returns a **0-entry edit
   script with exit status 0** - which scores as "this tool thinks nothing changed", i.e. a
   near-perfect result. `benchmark_other` neutralizes this per invocation by pointing
   GIT_CONFIG_GLOBAL and GIT_CONFIG_SYSTEM at /dev/null (`git_env`), for its own git calls as well
   as BDiff's. Anyone running BDiff by hand outside the harness must do the same.
2. **BDiff's `pyproject.toml` under-declares its dependencies.** It lists numpy and scipy;
   `bdiff/bdiff.py` also imports `rapidfuzz`. A plain `pip install .` yields a package that dies
   on `ModuleNotFoundError` at first use. `make install-bdiff` installs it explicitly.

BDiff also has no usable CLI for this purpose: `python -m bdiff a b` calls the library and
discards the result, printing nothing (verified 2026-08-23 - exit 0, empty stdout and stderr).
`assets/bdiff_driver.py`, embedded into `benchmark_other` via `include_str!`, exposes the edit
script and provides the batch mode below.

**Cold and warm timings, for BDiff as well as GumTree.** Importing BDiff costs ~394 ms (numpy,
scipy, rapidfuzz) against a ~12 ms bare interpreter, so a per-process wall-clock number for BDiff
is ~97% import overhead. `bdiff_ms` is the per-process cost a developer actually waits for;
`bdiff_warm_ms` times only the `bdiff.bdiff()` call inside one persistent interpreter. Quote which
one you mean, exactly as with `gumtree_ms` / `gumtree_warm_ms`.

**The git hunk parser has one trap worth knowing.** With `--unified=0`, a pure insertion is
`@@ -N,0 +M,K @@`, where before-side line `N` is the line the insertion lands *after* and is not
itself touched. Counting it shifts every git variant's before-side labels by one and produces
entirely plausible but wrong rates. `benchmark_other`'s unit tests pin both the `,0` case and a
differential check that `git_myers` agrees with `unix_diff`, which are independent implementations
of the same Myers family - a divergence there means the parser broke, not that a finding was made.

**GumTree build, 2026-08-23.** The previously-installed beta8 tree was gone and GitHub publishes
**no release asset for beta8** (only beta4 and beta3 ship zips), so beta8 was rebuilt from source
at tag `v4.0.0-beta8` with JDK 17 into `/var/tmp/tools/gumtree-4.0.0-beta8`. Verified by running
`gumtree list GENERATORS`: it registers `cpp-treesitter-ng` and `tsx-treesitter-ng` and no JSON
generator, which is the beta8 signature this repository has documented. Do not substitute the
beta4 zip: its generator set differs in both directions and its numbers are not comparable.

## `benchmark_accuracy.csv`

Same corpus and the same external-tool binaries, but accuracy only - no timing, so unlike
`benchmark_other.csv` this file is machine-independent and unaffected by load. Produced by
`cd research && make measure-tools-accuracy` (`benchmark_other --accuracy-csv`).

One row per fixture that has a `human_mapping.json` - the **whole** corpus, `handmade` included,
because the producer is corpus-wide by design ("refresh the whole file, never append", above).
Since 2026-09-09 the paper reports the sampled datasets alone, so the *readers*
(`benchmark_other_report.read_accuracy_rows`, `paper_variables.common_subset_concentration`) filter
by `_common.PAPER_DATASETS` - do not push that scoping into the producer, or the file stops being
the corpus-wide record the product side wants.

Columns: `sample.csv` provenance
(`language`, `repository`, `commit`, `path` - blank for the handmade fixtures that were never
promoted from a sample, 60 of 835 as of 2026-09-09), the denominators `total_lines`,
`total_nodes`, `total_leaf_nodes`, `total_visible_nodes`, and per tool a `_line_mismatches`,
`_node_mismatches`, `_leaf_node_mismatches`, `_visible_node_mismatches` and `_status` column.
Join to `src/test/data/sample.csv` on `solution == sample.csv:promoted_to`.

**What the node columns measure, and what they do not.** Both granularities are a *touched or
not* projection: for each line (or node), did the tool consider it changed, and does that agree
with the human mapping? A mismatch is one disagreement. This is deliberately **not** the
node-mapping fidelity metric `benchmark_optimal_solutions` reports for codediff, and the two
numbers must never be compared or mixed. An external tool parses its own tree and shares no node
identities with this codebase's AST, so "which node did this one become" cannot be asked of it at
all; "did you think this text changed" can be asked of everything. codediff is scored through the
identical projection here, which is what makes its column comparable to the tools' - and, by the
same token, not comparable to its own optimal-solutions figure.

**Three node denominators, deliberately.** `_node_mismatches` counts every AST node; because a
node counts as touched when a change lands anywhere inside it, that includes every ancestor of
every change up to the root, so the count partly reflects how deep a grammar's tree is.
`_leaf_node_mismatches` counts only childless nodes - non-nesting, and the granularity the
AST-aware tools actually report at. `_visible_node_mismatches` (added 2026-08-20) counts only
nodes carrying text of their own, per `codediff::diff::nodes::is_structurally_visible`, with
`total_visible_nodes` as its denominator. Report whichever you use explicitly; they are not
interchangeable.

**Visibility is structural: a property of the tree and the source bytes, not of any diff.** A node
is visible if it carries text of its own - a leaf, or an interior node with non-whitespace content
its children don't cover (`codediff::diff::nodes::is_structurally_visible`). Every tool is
therefore scored against the identical, fixed set of visible nodes, which is what makes the columns
comparable at all. An earlier version derived visibility from the renderer, which made the set move
with whichever diff produced it; that was replaced 2026-08-20 after it turned out a coarse diff
could score a perfect zero by rendering almost nothing. `_visible_node_mismatches` is a strict
*superset* of `_leaf_node_mismatches`: every leaf is visible, plus the interior nodes that carry
their own text (a comment whose marker is a separate child). They will therefore track each other
closely - the visible count is the leaf count plus the text-carrying interiors, not an independent
signal, so do not treat their agreement as a cross-check.

**The purely line-based tools have no node columns**, by construction rather than omission: Unix
diff and the four git algorithms report whole changed lines with no sub-line structure, so
projecting one onto nodes would mark every node on a changed line as changed. Their `_status` is
`line_only`.

**BDiff and `nvim -d` are not in that group, though they were scored as if they were until
2026-08-24.** Both match *lines* with libxdiff-class machinery, which is why they tie the git rows
on the line metric - but both then report which characters inside a line actually changed, and
that sub-line output was being parsed and discarded:

- BDiff's edit script carries a `str_diff` field on every `update`-family entry:
  `[before_ranges, after_ranges]`, each a list of **inclusive** `[start, end]` **character**
  offsets into that line (an empty `[]` = the side has nothing there, e.g. a pure insertion).
  Verified live 2026-08-24: `abcdefghij` -> `abcXYZfghij` gives `[[[3, 4]], [[3, 5]]]`. Caveat
  worth knowing when reading its node numbers: BDiff reports the **hull** of a line's changes, not
  each one - `one two three four` -> `onX two threX four` gives a single `[2, 12]` spanning the
  untouched middle - so it will over-report on lines with several separated edits.
- Neovim paints `DiffText` per column, readable only through `diff_hlID(lnum, col)`;
  `assets/nvim_diff_driver.lua` now records the runs of those columns rather than a per-line
  boolean.

Modes and lines with no sub-line detail (BDiff's `insert`/`delete`/`move`/`split`/`merge`/`copy`,
and any changed line Neovim paints `DiffAdd`/`DiffDelete` rather than `DiffText`) contribute their
whole line, on the same sides `bdiff_line_labels` documents - otherwise each tool would be scored
only on the subset it happens to call an update, which is not the same question.

This matters for what the two tables mean: the line-granularity table cannot distinguish a tool
that marks a whole changed line from one that marks the three characters that changed, and for
these two tools that difference is the only thing they add over `git diff`. Any claim about
sub-line quality has to come from the node columns, not the line ones.

**`_status` distinguishes an unscored cell from a zero.** `ok` = scored; `unsupported` = the tool
has no parser/generator for that language, so the fixture is out of its coverage (an empty cell,
never a 0, which would read as a perfect score); `error` = the tool was expected to handle the
language and failed; `line_only` = the node columns of a tool with no sub-line output at all (Unix
diff and the four git algorithms - *not* BDiff or `nvim -d`, see above).

**Tool versions are not recorded per row - record them here on every refresh.**

Refreshed **2026-09-09**, both CSVs, over 835 fixtures (775 of them in the paper's scope). Every
binary verified by running it, not by reading a path:

| tool | version | path |
| --- | --- | --- |
| GumTree | 4.0.0-beta8 | `/var/tmp/tools/gumtree-4.0.0-beta8/bin/gumtree` (`GUMTREE_BIN`) |
| difftastic | 0.70.0 | `/var/tmp/tools/bin/difft` (`DIFFT_BIN`) |
| diffsitter | 0.9.0 | `/var/tmp/tools/bin/diffsitter` (`DIFFSITTER_BIN`) |
| Neovim | 0.10.2 | `/var/tmp/tools/nvim-linux64/bin/nvim` (`NVIM_BIN`) |
| BDiff | 0.1.0 | `/var/tmp/bdiff-install/venv/bin/python` (`BDIFF_PYTHON`) |

`NVIM_BIN` is the fifth variable and was undocumented here until 2026-09-09. **Set all five before
either run.** A tool whose variable is unset is skipped with a note and exit 0, so a refresh done
without them produces a clean-looking CSV missing most of the nine tools, and `timing-report` will
regenerate the paper's macros from it. Smoke-test first with `--fixtures a,b` on a language
GumTree supports and check every `_status` reads `ok` or `line_only`.

The GumTree build in use is **4.0.0-beta8**, at `/var/tmp/tools/gumtree-4.0.0-beta8`, which is what
the paper's comparison section claims. Re-verified 2026-08-24 by running `gumtree list GENERATORS`
against that path: `cpp-treesitter-ng` and `tsx-treesitter-ng` present, no JSON generator. This resolves the version question that stood open here: the
beta4 tree under `/var/tmp/tools/` that earlier measurements ran against **is back on disk** at
`/var/tmp/tools/gumtree-4.0.0-beta4` (re-checked 2026-08-24; an earlier revision of this file said
it was gone). Do not point `GUMTREE_BIN` at it: its generator set differs from beta8's in both
directions, so it fails no louder than producing a different scored subset. The whole generator
table was re-verified against beta8 on 2026-08-20, entry by entry, by running each one on a real
fixture pair rather than reading `gumtree list GENERATORS`.

**The beta4 -> beta8 change moves GumTree's coverage in both directions**, so its scored subset is
not comparable across that boundary:

- **C++ and TSX gain support.** beta8 registers `cpp-treesitter-ng` and `tsx-treesitter-ng`, which
  beta4 does not; 22 C++ and 19 TSX fixtures enter GumTree's scored set.
- **JSON loses support.** beta4 registered `json-jackson`; beta8 does not, and running it errors
  out on argument parsing. 18 JSON fixtures leave the scored set. beta8's `gen.json` package
  registers only `xml-jsoup`.

**GumTree coverage was substantially wrong before 2026-08-20, in both directions.** Any GumTree
number from a run before that date is measured on a non-random 48% of the corpus and should not
be quoted:

- **104 of the 200 `unsupported` fixtures were not unsupported.** beta4 ships working generators
  for PHP, Ruby, Swift, R, JSON, XML and YAML; `gumtree_generator` simply didn't list them, so
  whole language families were silently dropped. All seven are now mapped, each verified against
  a real fixture pair from this corpus (a `textdiff -f JSON` run producing a non-empty `matches`
  array) rather than trusted from `gumtree list GENERATORS` alone.
- **`cpp-treesitter-ng` does not exist in beta4**, but the table mapped C++ to it, so all 21 C++
  fixtures counted as `error` - 21 of the 26 errors in the 2026-08-19 run. C++ was dropped from
  the table on that basis, and is back only now that the installed build genuinely ships the
  generator and it has been run. Verify by running, not by reading the generator list - that list
  is what made this look supported in the first place.
- Still genuinely unsupported by beta8, and correctly absent: HTML, LUA, Vimscript, ShellScript,
  Scala.

**GumTree's tree is not codediff's tree, and how far apart they are is per-language.** Node
counts on one fixture's before side, GumTree vs codediff: java-jdt 512/997 (1.95x),
java-treesitter-ng 585/997, python 6708/10314 (1.54x), rust 810/1344 (1.66x), kotlin 1330/1792
(1.35x), go 174/233 (1.34x) - but js 51/51, ruby 1427/1427, cs 2280/2280, c 23971/23969, php
34779/34771, i.e. **node-for-node identical**. GumTree's tree-sitter-ng bindings reuse the same
grammars and keep the same tokens; the divergent cases are where the generator is a different
parser altogether (Java's default is Eclipse JDT, which also emits synthetic nodes such as
`METHOD_INVOCATION_ARGUMENTS`) or a differently-versioned grammar. This is why the node columns
are a touched/not projection rather than a mapping comparison - but note that a real
mapping-fidelity comparison *would* be defensible for the 1.00x languages, since GumTree does
emit a full node-to-node mapping (`textdiff -f JSON`'s `matches` array covers intermediate nodes,
not just the edit script, with real byte offsets). difftastic and diffsitter emit no node
correspondences at all at any granularity, so they could never be included in such a comparison.

## astdiff_oracle_defects4j.csv (added 2026-09-11)

The first accuracy number for codediff against ground truth this project did not write. Produced
by `benchmark_astdiff_oracle` (`cd research && make measure-astdiff-oracle`) from two external
inputs fetched by the scripts in `research/external/` (see its README for the datasets):

* the Alikhanifard & Tsantalis AST node-mapping oracle, Defects4J half - RefactoringMiner
  `9d8743c0f5de2966ce1ae4df5ba778630c97a129` (2026-09-10), `src/test/resources/astDiff/defects4j/`,
  800 cases / 996 compilation units, 3,031,434 mapping records;
* the source files those records index into, from Falleri & Martinez' ICSE 2024 replication
  package (Zenodo 10474674), `dataset/defects4j/{before,after}/`.

One row per compilation unit: how many oracle records it held, how many resolved to tree-sitter
span pairs, and TP/FP/FN at the paper's two granularities (`all` = statement + sub-expression,
Table 12; `statement` = Table 11), plus codediff's wall-clock. `problematic` carries the oracle's
own `cases-problematic.json` flag (102 of the 800) so the two populations can be read apart.

**How a JDT mapping and a tree-sitter mapping are compared is the whole measurement**, and it is
documented at the top of `src/bin/benchmark_astdiff_oracle.rs`. In one paragraph: a JDT node and a
tree-sitter node are the same node when their byte spans agree (after converting JDT's UTF-16
offsets; 3 of the 1046 files are non-ASCII), with four span tolerances for systematic boundary
differences (a JDT body declaration starts at its Javadoc; `;`/`:` on `for` initialisers and
`case` labels; `METHOD_INVOCATION_ARGUMENTS` without its parentheses). Mappings under unchanged
program elements are excluded on both sides, as in the paper. A codediff pair is judged only when
the oracle maps one of its spans, or its node kind is one the oracle models one-to-one (a
data-driven whitelist: 83 kinds at `all`, 20 at `statement`) *and* the node moved or changed.

**Resolution: 78.02% of oracle records** resolve to a tree-sitter span pair. Of the 22% that do
not, 336,000 (all but ~6,000) are inside Javadoc and comments, which JDT parses into `TagElement`/
`TextElement` subtrees and tree-sitter leaves as one `block_comment`; excluding those, resolution
is **87.78%**. The remaining unresolved kinds are `SingleVariableDeclaration` (1,959),
`ArrayType`/`Dimension`/`VARARGS_TYPE` (~3,000 - JDT's type spans for `int[] x` and `String...`
differ from tree-sitter's), `METHOD_INVOCATION_ARGUMENTS` (2,221 - empty argument lists, where the
synthetic span is zero-width) and `SwitchCase` (349). Quote the resolution rate next to any
precision/recall from this file.

**Result (2026-09-11, codediff v0.0.13 at commit c091eea1 + this tool):**

| population / granularity | cases | oracle pairs scored | precision | recall | perfect-diff rate |
|---|---|---|---|---|---|
| all 800 / statement + sub-expression | 800 | 242,749 | 99.42% | 98.86% | 54.1% |
| all 800 / statement | 800 | 40,024 | 99.46% | 98.67% | 80.8% |
| cases.json (698) / statement + sub-expression | 698 | 198,572 | 99.57% | 99.42% | 59.0% |
| cases-problematic.json (102) / statement + sub-expression | 102 | 44,177 | 98.76% | 96.35% | 20.6% |

Against the paper's Table 12/14 (Defects4J, statement + sub-expression): RefactoringMiner 3.0
99.7 / 99.3, perfect 85.9%; GumTree 3.0 simple 98.4 / 97.8, perfect 63.3%; GumTree 3.0 greedy
97.5 / 93.1, perfect 18.1%. At statement level (Table 11/13): RM 99.8 / 99.6, perfect 89.4%;
GumTree simple 99.1 / 98.5, perfect 72.4%. So codediff's precision and recall sit between GumTree
simple and RefactoringMiner at both granularities; its perfect-diff rate beats GumTree simple at
statement level (80.8 against 72.4) and trails it at sub-expression level (54.1 against 63.3).

**Read those comparisons with two caveats.** (1) The paper's numbers were computed on JDT trees,
ours on tree-sitter trees through the span equality above; the 12% of non-comment oracle records
we cannot resolve are excluded from our denominator and not from theirs. (2) Our judged set of
codediff pairs is conservative by construction - a codediff pairing of a deleted node with an
inserted one is only counted as a false positive when its kind is whitelisted, and `identifier`
(ratio 0.916) and `binary_expression` (0.942, JDT's flat n-ary `InfixExpression` against
tree-sitter's nested pairs) fall below the 0.95 threshold. Both caveats push our precision up and
neither is easy to remove without a JDT parse.

**Where the errors are.** 429 of 996 files are imperfect, 230 of them by 1-3 pairs. The top 15
files carry 43% of all FP+FN and the top 50 carry 65%; the worst three (Closure-157
`CodeGenerator`, 455 FN / 0 FP; Closure-148 `SourceMap`, 325 FN; Time-23 `DateTimeZone`, 140 FP /
143 FN) are all in the oracle's own `problematic` list. Per project the FP+FN rate ranges from
0.28% (Gson) to 6.42% (Time). Rows are only comparable within one run; refresh the whole file.
