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

**Unix diff has no node columns**, by construction rather than omission: it reports whole changed
lines with no sub-line structure, so projecting it onto nodes would mark every node on a changed
line as changed. Its `_status` is `line_only`.

**`_status` distinguishes an unscored cell from a zero.** `ok` = scored; `unsupported` = the tool
has no parser/generator for that language, so the fixture is out of its coverage (an empty cell,
never a 0, which would read as a perfect score); `error` = the tool was expected to handle the
language and failed; `line_only` = Unix diff's node columns.

**Tool versions are not recorded per row - record them here on every refresh.** The GumTree build
in use is **4.0.0-beta8**, at `/var/tmp/gumtree-installed/gumtree-4.0.0-beta8`, which is what the
paper's comparison section claims. This resolves the version question that stood open here: the
beta4 tree under `/var/tmp/tools/` that earlier measurements ran against no longer exists on disk,
so the whole generator table was re-verified against beta8 on 2026-08-20, entry by entry, by
running each one on a real fixture pair rather than reading `gumtree list GENERATORS`.

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
