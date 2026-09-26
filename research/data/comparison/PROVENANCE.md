# Provenance

`benchmark_other.csv` is measured against the ground-truth fixtures in `src/test/data/diffs/`
(same fixture set as `../quality/`, not the sampled corpus), by `benchmark_other` with external
tool binaries supplied via GUMTREE_BIN / DIFFT_BIN / DIFFSITTER_BIN / BDIFF_PYTHON / NVIM_BIN /
SRCDIFF_BIN. Rows are only
comparable within one run: tool versions and machine are not recorded per row, so refresh the
whole file, never append to it.

## srcDiff (added 2026-09-26)

**`srcdiff`** - srcDiff (Decker, Collard, Volkert and Maletic, TOSEM 2020), built from
<https://github.com/srcDiff/srcDiff> into `/var/tmp/srcdiff-install` by `make install-srcdiff`,
which pins both commits. It parses C, C++, C#, Java and Python through srcML and writes one merged
srcML document; `src/bin/benchmark_other/srcdiff.rs` walks it with a cursor per side and trims
whitespace off each changed run, the same reading the other AST tools get.

**Three traps, all of which the adapter now catches or undoes.**

1. **srcDiff's tip needs an unreleased srcML.** It calls `srcml_unit_get_archive`, which no srcML
   release has (1.1.0, 2025-08, is the latest), so srcML is built from its development branch too.
2. **srcML rewrites the file it reads**: it drops a leading byte-order mark and the `\r` of every
   `\r\n` (JFreeChart and microsoft/terminal fixtures, several C# ones). Offsets over its text
   are not offsets over the file. The adapter reassembles both sides from the XML, compares them
   to the input as srcML read it, and maps offsets back; anything else that fails to reassemble
   is scored `error`, never silently.
3. **srcDiff can exit 0 with empty output.** On two Python fixtures it prints
   `vector::_M_range_check` to stderr and writes an XML declaration and nothing else, which would
   score as "nothing changed" - a near-perfect result on a small change. The reassembly check turns
   it into `error`. With two signal kills and one `Fatal Error Occurred`, that is the 5 `error`
   rows, all Python.

`-t UTF-8` is passed explicitly: srcDiff's default source encoding is ISO-8859-1, which would split
every non-ASCII character in two and shift every offset after it.

## Text-based tools

Eight tools are scored, in eleven configurations: Unix `diff`, `git diff` under four algorithms,
BDiff and `nvim -d` (text-based), and GumTree, difftastic, diffsitter and srcDiff (AST-aware). The
text-based ones beyond Unix `diff`:

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

  Its line pass is libxdiff, so its line set is close to `git_myers`' - but no closer than
  `git_myers` is to `unix_diff`, which get separate rows, and "redundant at the granularity we
  happen to measure" is a claim about the metric rather than about the tool. Over 486 fixtures it
  differs from `git_myers` on 13 - better on 6, worse on 7, pooled rate 1.125% against 1.123%.

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

**GumTree build.** GitHub publishes **no release asset for beta8** (only beta4 and beta3 ship
zips), so beta8 is built from source at tag `v4.0.0-beta8` with JDK 17, into
`/var/tmp/gumtree-installed/gumtree-4.0.0-beta8`. Verify it by running `gumtree list GENERATORS`:
beta8 registers `cpp-treesitter-ng` and `tsx-treesitter-ng` and no JSON generator. Do not
substitute the beta4 zip: its generator set differs in both directions (no C++ or TSX, but JSON),
so GumTree's scored subset is not comparable across the two.

## `benchmark_accuracy.csv`

Same corpus and the same external-tool binaries, but accuracy only - no timing, so unlike
`benchmark_other.csv` this file is machine-independent and unaffected by load. Produced by
`cd research && make measure-tools-accuracy` (`benchmark_other --accuracy-csv`).

One row per fixture that has a `human_mapping.json` - the **whole** corpus, `handmade` included,
because the producer is corpus-wide by design ("refresh the whole file, never append", above).
Since 2026-09-09 the paper reports the sampled datasets alone, so the *readers*
(`benchmark_other_report.read_accuracy_rows`, `paper_variables.common_subset_concentration`) filter
by `_common.PAPER_DATASETS` - do not push that scoping into the producer, or the file stops being
the corpus-wide record the product side wants. `defects4j` joined `PAPER_DATASETS` on 2026-09-16,
so that filter now drops only `handmade`.

Columns: `sample.csv` provenance
(`language`, `repository`, `commit`, `path` - blank for the handmade fixtures that were never
promoted from a sample, and for the Defects4J units, which come from an external oracle rather
than from this project's sampling: 175 of 1116 as of 2026-09-16 - 60 handmade, 113 Defects4J, and
the two fixtures added by hand that `sampling_provenance` names), the denominators `total_lines`,
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

Refreshed **2026-09-26**, both CSVs, over 1278 fixtures (1217 of them in the paper's scope), to add
srcDiff - 162 fixtures more than 2026-09-16, almost all of them newly solved Defects4J units (274
in scope, from 113). Same binaries as below for the other five, re-verified by running each, plus:

| tool | version | path |
| --- | --- | --- |
| srcDiff | `ef42b33` (reports 0.1.0) against srcML `c9f0014` (reports 1.1.0) | `/var/tmp/srcdiff-install/srcDiff/build/bin/srcdiff` (`SRCDIFF_BIN`) |

The status counts of the five older tools are unchanged apart from the new fixtures: GumTree still
11 `error` and 201 `unsupported`, difftastic 3 and 32, diffsitter 0 and 336, the same fixtures as
before. srcDiff: 535 `ok`, 5 `error`, 738 `unsupported`.

**The timing run needs more open files than a systemd unit allows by default.** `gumtree_warm_batch`
keeps both temp files of every fixture open until the persistent JVM has read them all - about
2,560 at this corpus size, past the 1,024 soft limit a `systemd-run --user` unit starts with, so the
first attempt died with `Too many open files` before timing anything. A login shell here has a
limit of 1,048,576, which is why no earlier refresh, all run from a shell, hit it. Run it with
`-p LimitNOFILE=1048576`, or from a shell.

**Timing was measured with the machine mostly idle, not verifiably idle throughout** (load average
0.3 when the accuracy half started, around 1.5 between the halves). The line-based tools' maxima
jumped - Unix `diff` from 27.8 to 246 ms, `git` (Myers) from 72 to 138 ms - on medians that did not
move, which is a scheduling hiccup, not the tools. Read the Max column as noisy.

Previously refreshed **2026-09-16**, both CSVs, over 1116 fixtures (1056 of them in the paper's scope) -
the pass that added `defects4j` to `_common.PAPER_DATASETS`, so 113 solved Defects4J compilation
units enter both files. Every binary verified by running it, not by reading a path:

| tool | version | path |
| --- | --- | --- |
| GumTree | 4.0.0-beta8 | `/var/tmp/gumtree-installed/gumtree-4.0.0-beta8/bin/gumtree` (`GUMTREE_BIN`) |
| difftastic | 0.69.0 | `/var/tmp/codediff-tools/bin/difft` (`DIFFT_BIN`) |
| diffsitter | 0.9.0 | `/var/tmp/codediff-tools/bin/diffsitter` (`DIFFSITTER_BIN`) |
| Neovim | 0.11.4 | `/opt/nvim-linux-x86_64/bin/nvim` (`NVIM_BIN`) |
| BDiff | 0.1.0 | `/var/tmp/bdiff-install/venv/bin/python` (`BDIFF_PYTHON`) |

A run that scores none of the fixtures still exits 0, so read the `[i/N]` progress line before
trusting a refresh.

**Set all six tool variables before either run** (`GUMTREE_BIN`, `DIFFT_BIN`, `DIFFSITTER_BIN`,
`BDIFF_PYTHON`, `NVIM_BIN`, `SRCDIFF_BIN`). A tool whose variable is unset is skipped with a note
and exit 0, so a refresh done without them produces a clean-looking CSV missing most of the tools, and `timing-report` will
regenerate the paper's macros from it. Smoke-test first with `--fixtures a,b` on a language
GumTree supports and check every `_status` reads `ok` or `line_only`.

GumTree's generator table was verified against beta8 entry by entry, by running each generator on
a real fixture pair (a `textdiff -f JSON` run producing a non-empty `matches` array) rather than
reading `gumtree list GENERATORS`. Still unsupported by beta8, and correctly absent: HTML, Lua,
Vimscript, ShellScript, Scala.

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

## astdiff_oracle_defects4j.csv

**Both oracle CSVs were last re-run on 2026-09-26** with the rest of the paper's refresh. The human
run (`astdiff_oracle_defects4j_human.csv`) covers 274 solved units in 231 cases and agrees with the
oracle at 99.65% precision / 99.63% recall, 380 disagreeing pairs in 52,726, and 39 of 8,762 at
statement level.

codediff's accuracy against ground truth this project did not write. Produced
by `benchmark_astdiff_oracle` (`cd research && make measure-astdiff-oracle`) from two external
inputs fetched by the scripts in `research/external/` (see its README for the datasets):

* the Alikhanifard & Tsantalis AST node-mapping oracle, Defects4J half - RefactoringMiner
  `6e926908a3fc9c03e290affe27564925679e0434` (fetched 2026-09-15), `src/test/resources/astDiff/defects4j/`,
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

**Result (2026-09-26):**

| population / granularity | cases | oracle pairs scored | precision | recall | perfect-diff rate |
|---|---|---|---|---|---|
| all 800 / statement + sub-expression | 800 | 242,749 | 99.43% | 98.87% | 55.4% |
| all 800 / statement | 800 | 40,024 | 99.46% | 98.67% | 80.8% |
| cases.json (698) / statement + sub-expression | 698 | 198,572 | 99.57% | 99.43% | 60.5% |
| cases-problematic.json (102) / statement + sub-expression | 102 | 44,177 | 98.76% | 96.35% | 20.6% |

Against the paper's Table 12/14 (Defects4J, statement + sub-expression): RefactoringMiner 3.0
99.7 / 99.3, perfect 85.9%; GumTree 3.0 simple 98.4 / 97.8, perfect 63.3%; GumTree 3.0 greedy
97.5 / 93.1, perfect 18.1%. At statement level (Table 11/13): RM 99.8 / 99.6, perfect 89.4%;
GumTree simple 99.1 / 98.5, perfect 72.4%. So codediff's precision and recall sit between GumTree
simple and RefactoringMiner at both granularities; its perfect-diff rate beats GumTree simple at
statement level (80.8 against 72.4) and trails it at sub-expression level (55.4 against 63.3).

**Read those comparisons with two caveats.** (1) The paper's numbers were computed on JDT trees,
ours on tree-sitter trees through the span equality above; the 12% of non-comment oracle records
we cannot resolve are excluded from our denominator and not from theirs. (2) Our judged set of
codediff pairs is conservative by construction - a codediff pairing of a deleted node with an
inserted one is only counted as a false positive when its kind is whitelisted, and `identifier`
(ratio 0.916) and `binary_expression` (0.942, JDT's flat n-ary `InfixExpression` against
tree-sitter's nested pairs) fall below the 0.95 threshold. Both caveats push our precision up and
neither is easy to remove without a JDT parse.

**Where the errors are.** 418 of 996 files are imperfect, 223 of them by 1-3 pairs. The top 15
files carry 43% of all FP+FN and the top 50 carry 65%; the worst three (Closure-157
`CodeGenerator`, 455 FN / 0 FP; Closure-148 `SourceMap`, 325 FN; Time-23 `DateTimeZone`, 140 FP /
143 FN) are all in the oracle's own `problematic` list. Per project the FP+FN rate ranges from
0.19% (Gson) to 6.42% (Time). Rows are only comparable within one run; refresh the whole file.
