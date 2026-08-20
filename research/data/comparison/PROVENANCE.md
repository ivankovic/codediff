# Provenance

`benchmark_other.csv` is measured against the ground-truth fixtures in `src/test/data/diffs/`
(same fixture set as `../quality/`, not the sampled corpus), by `benchmark_other` with external
tool binaries supplied via GUMTREE_BIN / DIFFT_BIN / DIFFSITTER_BIN. Rows are only comparable
within one run: tool versions and machine are not recorded per row, so refresh the whole file,
never append to it.

## `benchmark_accuracy.csv`

Same corpus and the same external-tool binaries, but accuracy only - no timing, so unlike
`benchmark_other.csv` this file is machine-independent and unaffected by load. Produced by
`cd research && make benchmark-accuracy` (`benchmark_other --accuracy-csv`).

One row per fixture that has a `human_mapping.json`. Columns: `sample.csv` provenance
(`language`, `repository`, `commit`, `path` - blank for the handmade fixtures that were never
promoted from a sample, 60 of 432 as of 2026-08-19), the denominators `total_lines`,
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
nodes that actually reach the screen when the diff is rendered, per
`codediff::diff::text::visible_node_ids`, with `total_visible_nodes` as its denominator. Report
whichever you use explicitly; they are not interchangeable.

**Visibility here is judged against the human mapping, not against any tool's own output.** This
deliberately differs from the visible-mismatch number `benchmark_optimal_solutions` reports,
which judges codediff's real diff by codediff's own rendering ("what does the user see right
now"). A comparative benchmark needs one fixed, tool-independent set of visible nodes, or each
tool gets a different denominator and the columns stop being comparable - and using *codediff's*
rendering as the basis would quietly privilege codediff. `_visible_node_mismatches` is also
**not** a synonym for `_leaf_node_mismatches`: the two differ in both directions (an `Identical`
leaf inside a terminal subtree is never reached by the renderer; a `MatchButNotIdentical`
container whose own content diverges emits its own span). If they come out close, that is a
coincidence, not a cross-check.

**Unix diff has no node columns**, by construction rather than omission: it reports whole changed
lines with no sub-line structure, so projecting it onto nodes would mark every node on a changed
line as changed. Its `_status` is `line_only`.

**`_status` distinguishes an unscored cell from a zero.** `ok` = scored; `unsupported` = the tool
has no parser/generator for that language, so the fixture is out of its coverage (an empty cell,
never a 0, which would read as a perfect score); `error` = the tool was expected to handle the
language and failed; `line_only` = Unix diff's node columns.

**Tool versions are not recorded per row - record them here on every refresh.** The GumTree build
under `/var/tmp/tools/` is **4.0.0-beta4**, which is *not* the 4.0.0-beta8 the paper's comparison
section claims. Resolve the version question before any refreshed GumTree number is quoted
anywhere.

**GumTree coverage was substantially wrong before 2026-08-20, in both directions.** Any GumTree
number from a run before that date is measured on a non-random 48% of the corpus and should not
be quoted:

- **104 of the 200 `unsupported` fixtures were not unsupported.** beta4 ships working generators
  for PHP, Ruby, Swift, R, JSON, XML and YAML; `gumtree_generator` simply didn't list them, so
  whole language families were silently dropped. All seven are now mapped, each verified against
  a real fixture pair from this corpus (a `textdiff -f JSON` run producing a non-empty `matches`
  array) rather than trusted from `gumtree list GENERATORS` alone.
- **`cpp-treesitter-ng` does not exist in beta4**, but the table mapped C++ to it, so all 21 C++
  fixtures counted as `error` - 21 of the 26 errors in the 2026-08-19 run. C++ is now absent from
  the table and reports the honest `unsupported`. Restore the mapping only against a GumTree
  build that actually ships a C++ generator, and verify by running it, not by reading the
  generator list - that list is what made this look supported in the first place.
- Still genuinely unsupported by beta4, and correctly absent: HTML, TSX, LUA, Vimscript,
  ShellScript, Scala.

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
