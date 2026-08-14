# Multi-file diff library: true unified matching (N before files, M after files)

**Status: tabled, not started.** Scoped and researched in full (every claim below was verified by
reading the actual code, not assumed), but the user stopped before implementation started - the
plan turned out larger than expected once fully scoped, and tests need to be written first. Pick
up at the "Implementation order" section when ready; step 2's identity-regression test is the
natural first thing to write.

## Context

Today `codediff`'s diff library (`src/diff.rs`, `src/diff/*`) diffs exactly one "before" `Code`
against exactly one "after" `Code`. The goal is to detect moves *across* file boundaries too (e.g.
a function split from one file into two), which needs a real multi-file matching pass, not just
running today's pipeline once per file pair. Two architectures were discussed:

- **Safe**: keep every file pair's diff completely independent, then run a *separate* cross-file
  move-reconciliation pass over the leftovers at the end.
- **Risky (chosen)**: unify matching itself so all before-files' nodes and all after-files' nodes
  are matched against each other in one pass, the same way phases already match within one file.

The risky option was explicitly chosen, scoped to **the diff library only** (not the TUI, not the
CLI, not git-commit ingestion), with existing tests changed as little as possible.

## Why this is tractable at all

Two encouraging facts, confirmed by reading the actual code (not assumed):

1. **The forest-matching engine already supports multiple roots on both sides simultaneously.**
   `resolve_forest`/`for_nodes`/`PostorderIndexer::build`/`ContainmentCtx::build`
   (`src/diff/apted/common.rs`) already take `Vec<usize>` root ids per side and run one shared
   tree-edit-distance computation - never indexing arrays by raw node id, always through a dense
   index the indexers assign internally. The only constraint: every id in a call's root list, on
   one side, must resolve through **one shared `&ASTMetadata`** for that side. So multi-file
   support is really "build one merged `ASTMetadata`/`NodeCache` per side" - the matching engine
   itself needs no changes.
2. **Most solvers' matching logic is already anchor/hash/name-based, not "same file" based.**
   `solve_moved_subtrees` (verified by full read) matches purely via `full_hash_to_node`/
   `node_to_parent`/`node_to_subtree_size` lookups - feed it merged, multi-file metadata and it
   searches across file boundaries automatically, no logic change needed.

## The blocker, and why it's solvable

`tree_sitter::Node::id()` is a raw arena/pointer value, unique only *within one parse* - not
across separate parses of different files (confirmed via the tree-sitter crate source; this
codebase's own `ASTNodeMetadata::start_byte` doc comment at `src/code.rs:269-274` already flags
this and recommends `start_byte`/`preorder_index` as the parse-stable alternative). Because ids are
pointer-derived (arbitrary magnitude, not small/dense), **arithmetic offsetting is unsafe** - the
fix is a translation table, not `file_index * STRIDE + local_id`.

The existing test suite already addresses nodes via `path_for_node`/`node_for_path` (kind +
sibling-index paths, `src/test/helper.rs`), never raw id values - so remapping ids is safe with
respect to "don't change existing tests," confirmed by the id-instability doc comment already
being a known, accepted property of this codebase.

**A second, more subtle blocker found by direct verification** (not just research - read the
function directly): `solve_bottom_up_expansion.rs`'s `common_descendant_count` (`:199-227`) uses
`preorder_index` as a **whole-tree range-containment check**:
```rust
let range_start = after_info.preorder_index;
let range_end = range_start + after_size;
// ... partner_info.preorder_index >= range_start && partner_info.preorder_index < range_end
```
This is only correct if the whole side was numbered by one contiguous preorder walk. Unlike
`node.id()`, `preorder_index` is a dense counter this codebase fully owns (not an arena pointer),
so it **can** be safely renumbered with a per-file *cumulative offset* during merge - it just must
not be copied forward unchanged, or file B's nodes will spuriously appear "contained" in file A's
subtrees. All other `preorder_index` uses (found via full-repo search) are pure sort-key tie-breaks
(`solve_bottom_up_expansion.rs`'s own candidate ordering, `solve_greedy_anchor_blocks.rs:230`,
`solve_syntax_aware_matching.rs:166/170`, three sites in `apted/common.rs`) and remain correct
under any per-file-offset renumbering.

Also confirmed: `ASTDiff::add_mapping`'s convention reserves id `0` for insert/delete - the merge's
dense id counter must start at **1**.

## Design

### 1. `ASTNodeMetadata` gains a `language` field

```rust
pub struct ASTNodeMetadata {
    pub kind: String,
    pub text: String,
    pub children: Vec<usize>,
    pub start_byte: usize,
    pub preorder_index: usize,
    pub language: Language,   // NEW
}
```
Populated identically to `kind`/`text` in `compute_node_info` (`src/code/metadata.rs`). Mechanical;
touches ~8 hand-built `ASTNodeMetadata { .. }` test literals (`src/diff/apted/common/tests.rs` and
others) with one added field each. Zero behavior change for single-language callers.

This is the prerequisite fix for a real gap found while researching: `UnitCostModel` and several
call sites (`common.rs`'s `classify_match`/`emit_match`/`subtree_del_cost`/
`update_context_supported`, `solve_moved_subtrees.rs:64`, `solve_identical_diagnostic_statements.rs`,
`solve_syntax_aware_matching.rs`) currently read **one blanket `before_metadata.language`** for a
whole call/pair, rather than the specific node(s) being judged. Every one of these call sites
already has the specific node's `ASTNodeMetadata` in hand - swap the blanket read for
`node.language` at each site (each already confirmed via reading `common.rs` and the solver files).
No behavior change for existing (single-language) tests; this only matters once two files can
genuinely differ in language, which is out of scope to validate for v1 (see "Explicitly out of
scope" below) but must not silently misbehave.

### 2. New module: id-translation merge (`src/diff/merge.rs`, new file)

```rust
/// Provenance + id-translation table for one side (before or after), produced by `merge_metadata`.
pub(crate) struct FileOrigins<'code> {
    files: Vec<&'code Code>,
    file_of: rustc_hash::FxHashMap<usize, usize>,       // merged id -> file index
    raw_to_merged: Vec<rustc_hash::FxHashMap<usize, usize>>, // per file: raw tree-sitter id -> merged id
}

impl<'code> FileOrigins<'code> {
    pub(crate) fn source_of(&self, merged_id: usize) -> &'code [u8];
    pub(crate) fn file_index_of(&self, merged_id: usize) -> Option<usize>;
    /// Translate a *freshly derived* raw id (from `.parent()`/`.children()` on a node already
    /// known to belong to `file_index`) into the merged id space.
    pub(crate) fn merged_id(&self, file_index: usize, raw_id: usize) -> Option<usize>;
}

pub(crate) struct MergedInput<'code> {
    pub(crate) before_metadata: ASTMetadata,
    pub(crate) after_metadata: ASTMetadata,
    pub(crate) node_cache: NodeCache,
    pub(crate) before_origins: FileOrigins<'code>,
    pub(crate) after_origins: FileOrigins<'code>,
    pub(crate) before_root_ids: Vec<usize>,  // one per before file, in the merged id space
    pub(crate) after_root_ids: Vec<usize>,
}

pub(crate) fn merge_metadata<'code>(before: &'code [Code], after: &'code [Code]) -> MergedInput<'code>;
```

**Identity property (load-bearing for backward compatibility)**: when a side has exactly 1 file,
that side's translation is the identity - merged ids equal that file's own raw tree-sitter ids
verbatim, `preorder_index` untouched, output byte-for-byte identical to today's single-file
`compute_ast_metadata`/`NodeCache::build`. This is what lets `Diff::from_code` become a one-line
wrapper with truly zero behavior change. **Verify this explicitly with a dedicated test** (below) -
it's the single biggest risk-concentration point in the plan, since `from_code` becomes
live-routed through the new merge machinery.

Per-side algorithm (single file: return `metadata_of`/`NodeCache::cache_for` unchanged, no
translation. Multiple files): for each file in order, get its already-computed `ASTMetadata` via
`metadata_of` (reused completely unchanged), assign each of its nodes a fresh id from a running
counter starting at 1, and rewrite into the merged `ASTMetadata`:
- `node_info.children`: translated. `preorder_index`: original + this file's cumulative node-count
  offset (the fix above). `language`: this file's `Code.metadata.language`. `kind`/`text`/
  `start_byte`: copied verbatim (pure per-node values).
- Every `node_to_*_hash` map: key translated, hash value copied verbatim (hashes don't depend on
  node id - confirmed via `code/hash.rs`).
- Every `*_hash_to_node` map: **extend, don't overwrite** (two files can share a hash - duplicated
  boilerplate) - append translated ids in file order.
- `node_to_subtree_size`/`node_to_depth`: key translated, value copied.
- `node_to_widest_subtree_node`: key translated; value is `(count, node_id)` - translate the id too.
- `node_to_parent`: key and value both translated.
- `reference_nodes_ordered`: concatenate translated lists, then **re-sort the whole thing** by
  subtree size (check `discover_reference_nodes`'s exact tie-break and replicate it, so ordering
  stays deterministic).
- Build the merged `NodeCache` side by inserting each file's `NodeCache::cache_for(code)` entries
  under translated ids.

### 3. Solver migration - two groups, verified by reading all 7 files in full

**Group A** (`solve_hash_descent.rs`, `solve_bottom_up_expansion.rs`, `solve_moved_subtrees.rs`,
`solve_greedy_anchor_blocks.rs`) - confirmed via grep for `.children(&mut`/`.parent()`/`.walk()`/
`utf8_text(`/`.contents.`: none in production code. Drop-in signature change:
```rust
// before: pub fn solve(before: &Code, after: &Code, node_cache: &NodeCache, diff: &mut ASTDiff)
// after:
pub fn solve(before_metadata: &ASTMetadata, after_metadata: &ASTMetadata, node_cache: &NodeCache, diff: &mut ASTDiff)
```
(drop the internal `metadata_of(before)`/`metadata_of(after)` calls; take metadata as a parameter).
Lowest risk - migrate first.

**Group B** (`solve_comment_nodes.rs`, `solve_identical_diagnostic_statements.rs`,
`solve_syntax_aware_matching.rs`, `solve_large_flat_subtrees.rs`)
- these read raw source bytes (`.contents.as_bytes()`) and/or call `.id()` on nodes freshly derived
  via `.parent()`/`.children()` (not already in `node_info`), so they need `FileOrigins` threaded
  through:
```rust
pub fn solve(
    before_metadata: &ASTMetadata, after_metadata: &ASTMetadata, node_cache: &NodeCache,
    before_origins: &FileOrigins, after_origins: &FileOrigins, diff: &mut ASTDiff,
)
```
Two mechanical changes ripple through this group and the shared helpers they call (`nodes.rs`'s
`map_identical_descendants`, `solve_syntax_aware_matching.rs`'s named-reference-group resolution):
1. `before.contents.as_bytes()` → `before_origins.source_of(node_id)` at the point a specific
   node's `.utf8_text()` is taken (every current call is on that node's own span - confirmed no
   arbitrary cross-node byte ranges exist).
2. Root traversal (`before.ast.as_ref().unwrap().root_node()`) becomes a loop over
   `before_root_ids`, each resolved to a `Node` via `node_cache.before.get(&root_id)`.
3. Any bare `.id()` on a node derived via `.parent()`/`.children()` becomes
   `origins.merged_id(file_idx, raw_id)` - `file_idx` stays constant for one recursive descent
   (traversal never crosses tree boundaries), so this is a parameter thread-through, not a rewrite
   of the recursion.

No solver's actual matching *logic* (which candidates pair, cost function, ordering) changes -
only how it looks up metadata/text/ids. Migrate one file at a time, full suite after each.

### 4. `apted::for_roots_fallback` needs the same multi-root generalization - found via direct read, not assumed

`for_roots_fallback` (`src/diff/apted/mod.rs:39-54`, the `DiffMode::Fast` cheap-fallback path) is
currently hardcoded to exactly one root per side:
```rust
let before_root_id = before.ast.as_ref().unwrap().root_node().id();
let after_root_id = after.ast.as_ref().unwrap().root_node().id();
common::resolve_residual_forest_via_myers_lcs(&before_metadata, &after_metadata, before_root_id, after_root_id, source, diff);
```
`resolve_residual_forest_via_myers_lcs` (`apted/common.rs:1292`) and the `maximal_unmatched_roots`
helper it calls (`:1252`) both take a *single* root id, not a list. Without fixing this, the
`DiffMode::Fast` guard path would silently only look at one file per side once multi-file residuals
get large - a real correctness gap, not just a missed optimization. Fix: change both functions to
take `&[usize]` root ids per side, looping `maximal_unmatched_roots` once per root and
concatenating results (in file order) before feeding the combined sequence to `myers_lcs` - the
same "singular → `Vec`" pattern already used everywhere else in this plan.

### 5. Phase 6/7 - confirmed to need no further changes

Phase 6 becomes one `apted::for_nodes(&merged.before_metadata, &merged.after_metadata,
merged.before_root_ids, merged.after_root_ids, Algorithm::Apted, "final_pass", &mut ast_diff)` call
- `for_nodes`/`resolve_forest`/`ContainmentCtx`/`PostorderIndexer` need zero changes beyond the
language fix in step 1 (confirmed by reading `common.rs:2192-2466` in full). Phase 7
(`solve_moved_subtrees`) needs only the Group-A signature change - its hash+size+kind matching
logic already searches across whatever `full_hash_to_node`/`node_to_parent` cover, which after
merge is every file.

### 6. Public API

```rust
impl Diff {
    pub fn from_files(before: &[Code], after: &[Code]) -> Self {
        Self::from_files_with_config(before, after, &HeuristicConfig::default())
    }
    pub fn from_files_with_config(before: &[Code], after: &[Code], config: &HeuristicConfig) -> Self {
        Self::pending_files_with_config(before, after, config).finish(DiffMode::Fast)
    }
    pub fn pending_files<'code>(before: &'code [Code], after: &'code [Code]) -> PendingDiff<'code> {
        Self::pending_files_with_config(before, after, &HeuristicConfig::default())
    }
    pub fn pending_files_with_config<'code>(before: &'code [Code], after: &'code [Code], config: &HeuristicConfig) -> PendingDiff<'code> {
        let merged = merge_metadata(before, after);
        // phases 1-5, each solver now taking merged.before_metadata/after_metadata/node_cache/origins
        ...
    }

    // existing entry points become real one-line wrappers - the actual backward-compat mechanism:
    pub fn from_code(before: &Code, after: &Code) -> Self {
        Self::from_files(std::slice::from_ref(before), std::slice::from_ref(after))
    }
    pub fn pending<'code>(before: &'code Code, after: &'code Code) -> PendingDiff<'code> {
        Self::pending_files(std::slice::from_ref(before), std::slice::from_ref(after))
    }
    // from_code_with_config / pending_with_config: same pattern
}
```
`diff_code`/`diff_code_with_config` (module-level free functions, `src/diff.rs:803-812`) need no
change - they already just call `Diff::from_code(_with_config)`.

`PendingDiff<'code>`'s private fields change from `before: &'code Code, after: &'code Code,
node_cache: NodeCache` to holding `MergedInput<'code>` instead - invisible to every external
caller, since every field is already private and only reached via `looks_expensive()`/
`unmatched_counts()`/`finish()`, both of which keep working off `node_cache.before.len()`/etc.
unchanged.

### 7. New tests for the multi-file capability

- **Identity regression test (highest priority - proves the backward-compat mechanism itself)**:
  run the same fixture through `Diff::from_code(&before, &after)` and
  `Diff::from_files(&[before], &[after])`, assert `ASTDiff.mapping`/`before_node_map`/
  `after_node_map` are literally `==`.
- **Whole-function cross-file move**: `before = [file_a_with_fn, file_b_without_fn]`,
  `after = [file_a_without_fn, file_b_with_fn]` - assert the function matches across files instead
  of showing up as delete+insert. Direct multi-file analogue of `solve_moved_subtrees.rs`'s existing
  `moved_function_is_matched_not_deleted` single-file test.
- **Split across two after-files**: one before-file with a function; two after-files each holding
  half its statements - assert both halves match into their respective destination, not both
  becoming pure inserts.
- **Unrelated multi-file sanity check**: 3 before files + 3 after files with no real relationship -
  confirm nothing spuriously cross-matches (mirrors the existing `tiny_identical_statements_do_not_move` guard).
- **`merge_metadata` unit tests** (new `src/diff/merge.rs` test module): construct 2-3 small `Code`
  fixtures directly, assert no id collisions, `preorder_index` ranges are non-overlapping and
  monotonic per file, `*_hash_to_node` correctly unions duplicate hashes across files, and the
  single-file path matches `compute_ast_metadata`'s direct output byte-for-byte.

Existing `src/test/optimal_solutions/*.rs` fixtures are single-file and are left untouched - the
identity property means they need no changes.

## Implementation order (risk mitigation - run full suite after each step)

1. `ASTNodeMetadata.language` + fix all read sites (§1). Should be a no-op for existing tests.
2. `merge_metadata`/`FileOrigins` (§2), single-file identity path only. Add the identity-regression
   test here, before touching any solver.
3. Migrate Group A solvers one at a time (§3).
4. Migrate Group B solvers one at a time, smallest/most self-contained first
   (`solve_comment_nodes`, `solve_identical_diagnostic_statements`), most raw-traversal-heavy last
   (`solve_syntax_aware_matching`).
5. Fix `for_roots_fallback`/`resolve_residual_forest_via_myers_lcs` (§4), wire up
   `PendingDiff`/`finish` (§5/§6), make `from_code`/`pending` real wrappers.
6. Add the cross-file move/split/sanity tests (§7).

## Explicitly out of scope for this pass

- TUI, CLI, git-commit ingestion (per the original scoping).
- Validating true cross-language before/after diffing - the per-node language fix prevents
  silent misbehavior, but realistic use (splitting/moving code between files) is same-language;
  cross-language semantics for `is_reference`/`is_semantically_structural`/kind-matching across
  different grammars is a separate, larger question.
- Performance tuning for large N (many files). `EXPENSIVE_RESIDUAL_THRESHOLD`/`DiffMode::Fast`
  were calibrated for one file pair; merging many files' hash tables/forests may need its own
  threshold recalibration later, but that's a tuning pass, not a blocker for a first working version
  aimed at "a handful of related files."

## Critical files

- `src/diff.rs` - `Diff`/`PendingDiff`/`NodeCache` public API
- `src/code.rs` - `ASTMetadata`/`ASTNodeMetadata`
- `src/code/metadata.rs` - `compute_ast_metadata`/`compute_node_info`
- `src/diff/merge.rs` (new) - `merge_metadata`/`FileOrigins`
- `src/diff/apted/common.rs` - `UnitCostModel` language fix, `resolve_residual_forest_via_myers_lcs`
- `src/diff/apted/mod.rs` - `for_roots_fallback`
- `src/diff/nodes.rs` - `map_identical_descendants`
- `src/diff/solve_moved_subtrees.rs`, `solve_hash_descent.rs`, `solve_bottom_up_expansion.rs`,
  `solve_greedy_anchor_blocks.rs` (Group A)
- `src/diff/solve_comment_nodes.rs`, `solve_identical_diagnostic_statements.rs`,
  `solve_syntax_aware_matching.rs`, `solve_large_flat_subtrees.rs` (Group B)

## Verification

- `cargo test --release` after every step in the implementation order above - the existing suite
  (380+ tests) must stay green throughout, since the identity property means it's the primary
  regression guard for this whole change.
- The new identity-regression test is the direct proof the backward-compat mechanism works, not
  just an assumption.
- `cargo run --release --bin benchmark_optimal_solutions` before/after the full change - should be
  byte-for-byte identical (every existing fixture is single-file, so takes the identity path).
