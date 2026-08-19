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
`total_nodes`, `total_leaf_nodes`, and per tool a `_line_mismatches`, `_node_mismatches`,
`_leaf_node_mismatches` and `_status` column. Join to `src/test/data/sample.csv` on
`solution == sample.csv:promoted_to`.

**What the node columns measure, and what they do not.** Both granularities are a *touched or
not* projection: for each line (or node), did the tool consider it changed, and does that agree
with the human mapping? A mismatch is one disagreement. This is deliberately **not** the
node-mapping fidelity metric `benchmark_optimal_solutions` reports for codediff, and the two
numbers must never be compared or mixed. An external tool parses its own tree and shares no node
identities with this codebase's AST, so "which node did this one become" cannot be asked of it at
all; "did you think this text changed" can be asked of everything. codediff is scored through the
identical projection here, which is what makes its column comparable to the tools' - and, by the
same token, not comparable to its own optimal-solutions figure.

**Two node denominators, deliberately.** `_node_mismatches` counts every AST node; because a
node counts as touched when a change lands anywhere inside it, that includes every ancestor of
every change up to the root, so the count partly reflects how deep a grammar's tree is.
`_leaf_node_mismatches` counts only childless nodes - non-nesting, and the granularity the
AST-aware tools actually report at. Report whichever you use explicitly; they are not
interchangeable.

**Unix diff has no node columns**, by construction rather than omission: it reports whole changed
lines with no sub-line structure, so projecting it onto nodes would mark every node on a changed
line as changed. Its `_status` is `line_only`.

**`_status` distinguishes an unscored cell from a zero.** `ok` = scored; `unsupported` = the tool
has no parser/generator for that language, so the fixture is out of its coverage (an empty cell,
never a 0, which would read as a perfect score); `error` = the tool was expected to handle the
language and failed; `line_only` = Unix diff's node columns.

**Tool versions are not recorded per row - record them here on every refresh.** The GumTree build
under `/var/tmp/tools/` is **4.0.0-beta4**, which is *not* the 4.0.0-beta8 the paper's comparison
section and `gumtree_generator`'s doc comment claim. beta4 also has **no `cpp-treesitter-ng`
generator** (verified via `gumtree list generators` and by running it), so every C++ fixture
errors under it even though `ExternalTool::supports` claims coverage. Resolve the version
question before any refreshed GumTree number is quoted anywhere.
