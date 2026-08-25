# Code Health Review — 2026-07-06

# HUMAN PLAYTESTING

* n/p should always say "There are changes in the other panel, would you like me to move you to the
  other panel" when you try to go beyond the last diff in the current panel and the other panel
  actually has changes. If the files are identical, a popup "Files are identical" is good.

Scope: duplicate code, code length, refactoring/generalization opportunities, structure,
readability. Ordered by expected payoff within each section. Line numbers are as of commit
f5e78e0.

## Status (2026-07-07)

Implemented in the working tree, verified by the full test suite (266 passed; only the two
pre-existing failures remain: `rust_algorithm_change` / `rust_turbopack_module_rule`
`matches_human_solution`) and by `benchmark_optimal_solutions` (178 mismatches / 13 unsolved —
byte-identical to a clean HEAD baseline run):

- **1.1 + 1.2 + 1.3** — `code::metadata::metadata_of` (Cow) replaced all 18 clone stanzas; the
  twin passes are now thin wrappers over `diff::hash_tree_matching::solve` (spec = hash accessor +
  classifier + reasons); the O(n) `mapping.iter().any` scans became O(1) `before_node_map`
  lookups; the identical pass got the structural pass's unclaimed-duplicate fix, and the pinning
  test now asserts one-to-one duplicate pairing.
- **1.4** — `NodeCache::build` folded into one `cache_for` helper; the `unsafe` transmute now has
  a single home.
- **1.5 + 1.6 + 1.9(blob)** — `stats::filesystem::find_git_repositories` (sorted; `commit_stats`
  is now deterministic), generic `stats::sampling::Reservoir<T>`, `stats::git::blob_bytes`.
- **§4 lints** — the four `spf_a` dead-store initializers removed; rustc's definite-assignment
  check proves the values were never read. Build is warning-free.
- **§4 typos** — `memo`, `unmatched`, forest/theoretically/mapping/maps/languages/its, plus
  `Therefore`/`something`/`fields` in code.rs/metadata.rs.

Not yet done (deliberately): the module splits of `human_solver.rs` / `apted/common.rs` (§2) and
the before/after side-parameterization (1.7/1.8) — the next candidates, per §5's order of attack.

---

## 1. Duplicate code

### 1.1 Metadata-clone boilerplate in every solver pass (also a real perf cost)

Every pass in the pipeline opens with the same 8-line stanza, once per side:

```rust
let before_metadata = before.metadata.ast_metadata.clone().unwrap_or_else(|| {
    crate::code::metadata::compute_ast_metadata(before).unwrap_or_default()
});
```

Sites (9 files, 18 clones): `solve_identical_trees.rs:37`, `solve_structurally_identical_trees.rs:42`,
`solve_semantically_structural_nodes.rs:115` **and** `:258` (twice in one file),
`solve_similar_flow_control.rs:56`, `solve_identical_diagnostic_statements.rs:62`,
`solve_moved_subtrees.rs:66`, `apted/common.rs:2293`, `test/helper/optimal_iud.rs:160`.

This is not just repetition: `ASTMetadata` holds several whole-tree `HashMap`s
(`node_info`, `node_to_full_hash`, `full_hash_to_node`, structural-hash maps, …), so a single
`Diff::from_code` run deep-copies both sides' metadata ~7 times each. The comment at each site
("We clone to avoid lifetime issues") concedes the clone is incidental.

**Suggestion:** compute/ensure metadata once in `Diff::from_code` and pass `&ASTMetadata` for each
side into every pass (they already all share the `solve(before, after, &node_cache, &mut ast_diff)`
signature — extend it to take the two metadata refs). If a standalone entry point still needs the
fallback, one shared helper `fn metadata_of(code: &Code) -> Cow<'_, ASTMetadata>` replaces all 18
stanzas.

### 1.2 `solve_identical_trees` vs `solve_structurally_identical_trees` are near-clones with drift

The two passes share ~80% of their body line-for-line: same metadata stanza, same
reference-node loop, same "skip already mapped" check, same paired stack-descent matching children
by position and kind, same redundant `{ let after_node_id = matching_after_node.id(); … }` shadow
block. They differ only in (a) which hash map they consult (full vs structural), (b) how the
root/child operation is classified (always `Identical` vs text-compare → `Identical`/`Update`),
and (c) duplicate handling — and (c) is *accidental* drift, not a design difference:

- `solve_identical_trees.rs:91` takes `after_node_ids.iter().next()` — the documented
  duplicate-collapse bug (all N before-duplicates map onto one after-node; there's a 40-line
  comment plus a pinning test for it).
- `solve_structurally_identical_trees.rs:88` already fixed the same problem with
  `.find(|&&id| !diff.after_node_map.contains_key(&id))`.

**Suggestion:** extract one generic pass parameterized by (hash-map accessor, operation
classifier) — the structural pass's claimed-node filter then fixes the identical pass's TODO for
free. If the full extraction feels heavy, at minimum port the `.find(unclaimed)` fix and extract
the shared child-descent loop.

### 1.3 O(n) linear scans over `diff.mapping` inside per-node loops

Both twin passes ask "is this before-node already mapped?" via

```rust
diff.mapping.iter().any(|((before_id, _), _)| *before_id == before_node_id)
```

(`solve_identical_trees.rs:52` and `:133`; `solve_structurally_identical_trees.rs:57` and `:151`).
That is a full scan of the mapping table per reference node *and per descended child*, i.e.
O(nodes × mappings) per pass, when the O(1) answer already exists:
`diff.before_node_map.contains_key(&before_node_id)` (`add_mapping` keeps `before_node_map` in
lockstep with `mapping`, including delete/insert null entries, so the two checks are equivalent).
Fix falls out automatically if 1.2's extraction happens.

### 1.4 `NodeCache::build` — before/after bodies are copy-pasted verbatim

`diff.rs:63–117`: the two ~25-line closures building `before_cache` and `after_cache` are
byte-identical except for the variable they capture. Extract
`fn cache_for(code: &Code) -> HashMap<usize, Node<'static>>` (the `unsafe` transmute and its
SAFETY comment then live in exactly one place, which is also better for auditing the documented
soundness invariant).

### 1.5 `find_git_repositories` — three copies, one behaviorally drifted

- `sample_code_pairs.rs:122` and `sample_test_diffs.rs:129`: **byte-identical** (verified with
  `diff`), including the doc comment "Sorted for reproducible traversal order across runs".
- `commit_stats.rs:46`: older variant — extra `println!`s, and **no sorting**, so its traversal
  order is filesystem-dependent even though the two newer copies fixed exactly that.

**Suggestion:** the crate already has a library (`src/lib.rs`); move one canonical, sorted version
into a small shared module (e.g. `stats::filesystem` already exists as a home for fs helpers) and
delete the copies. `commit_stats` silently becomes reproducible too.

### 1.6 `Reservoir` — two copies, identical modulo element type

`sample_code_pairs.rs:100` (`items: Vec<Candidate>`) and `sample_test_diffs.rs:107`
(`items: Vec<Row>`) implement the same reservoir sampling `offer` line-for-line. A generic
`Reservoir<T>` in the same shared module as 1.5 replaces both (and both binaries' duplicated
`reservoir_never_exceeds_capacity` tests can collapse into one).

### 1.7 Systematic before/after mirror pairs in `apted/common.rs`

Pairs that differ only in which side's maps/costs/operations they touch:

| Before-side | After-side | Lines |
|---|---|---|
| `emit_before_subtree` | `emit_after_subtree` | 772 / 813 |
| `add_delete_mappings` | `add_insert_mappings` | 885 / 918 |
| `subtree_del_cost` | `subtree_ins_cost` | 951 / 970 |
| `filter_before_nodes` | `filter_after_nodes` | 1176 / 1184 |
| `before_match_target` | `after_match_target` | 1202 / 1215 |
| `collect_before_subtree_targets` | `collect_after_subtree_targets` | 1249 / 1283 |

That's ~250 lines of mirrored logic. Notably, `engine.rs::spf_a` already demonstrates the
side-parameterized style this file could use (`path_is_before: bool` selecting
`(idx, meta, del/ins)` at the top, one body). A small `struct SideCtx { meta, decision,
has_match_below, node_map, unit_cost_fn, null_slot }` — or an enum implementing the six accessors —
would halve this block and, more importantly, remove the standing risk of fixing a bug on one side
only (the same failure mode `MEMORY`/`HANDOVER` already track for `kinds_update_allowed`'s three
call sites).

Related micro-duplication: `UnitCostModel { language: … }` is constructed inline at ~6 sites in
the emit/cost helpers (`common.rs:739, 787, 828, 895, 928`, …). `ResolveCtx` could carry one.

### 1.8 Before/after mirror pairs in `human_solver.rs`

Same pattern at TUI scale — each pair differs only in which cache/map/glyph it reads:
`status_before`/`status_after` (742/763), `algo_status_before`/`algo_status_after` (837/845),
`algo_disagrees_before`/`algo_disagrees_after` (867/879),
`advance_before_to_next_unmarked`/`advance_after_to_next_unmarked` (949/956),
`clear_before_descendants`/`clear_after_descendants` (1188/1200). A `Side` enum **already exists**
(line 1883) but is not used to unify any of these. The file already passes `status_fn` as a
function pointer in places (`fully_solved_nodes`, `advance_to_next_unmarked`) — extending `Side`
with `fn caches_match(&self, &Caches)`, `fn removed(&self, &Caches)`, `fn mark_kind(&self)`
accessors would fold each pair into one function.

### 1.9 Smaller twins

- `test/helper.rs`: `was_tree_added` / `was_tree_deleted` (235/253) and
  `was_node_added` / `was_node_deleted` (225/230) differ only in the `(0, id)` vs `(id, 0)` key —
  one helper taking a key-constructor closure covers all four.
- `diff/text.rs::ranges` (42–248): the `DeleteWithChildren`, `InsertWithChildren`, `Delete`,
  `Insert`, and `Update` match arms are five copies of the same
  "`right_limit()` + build `RangeMatch` with operation X" block (differing in operation and the
  leaf-only guard). A 10-line helper `fn emit_at_right_limit(op, node, …)` shrinks the 200-line
  function by nearly half and makes the two genuinely distinct arms (`Identical`'s move
  detection, default descend) stand out.
- `bin/benchmark_diff_pairs.rs::blob_content` vs `bin/materialize_test_diffs.rs::blob_text` —
  same git-blob-lookup shape; candidate for the shared bin-support module of 1.5/1.6.

---

## 2. Code length

Function-length outliers (non-test, measured by brace matching):

| Lines | Function | Verdict |
|---|---|---|
| 585 | `apted/engine.rs:498 spf_a` | Leave. Hand-tuned APTED port; prior review already ruled: profile before touching. |
| 298 | `test/helper/optimal_iud.rs:311 solve_with_slices` | Test-only oracle; splitting the 4-branch cost search per `AlgorithmChoice` arm would help, but low priority. |
| 294 | `bin/human_solver.rs:2540 handle_key` | Worth splitting — see below. |
| 268 | `bin/human_solver.rs:2841 handle_modal_key` | Same: one function per `Modal` variant. |
| 207 | `diff/text.rs:42 ranges` | Shrinks naturally via finding 1.9. |
| 204 | `test/helper/optimal_iud.rs:691 update_diff` | Same file/status as `solve_with_slices`. |
| 180 | `apted/common.rs:2076 resolve_forest` | Sequential phases with clear comments; acceptable, could be phase-functions if touched again. |
| 176+164 | `engine.rs compute_opt_strategy_post_l` / `_post_r` | Mirrored algorithm variants, same leave-alone rule as `spf_a`. |
| 163 | `solve_structurally_identical_trees.rs:35 solve` | Shrinks via 1.2. |

File-length outliers:

- **`bin/human_solver.rs` (4,298 lines)** is the largest file in the repo and is a single-file
  binary with ~900 lines of tests. It already has clean internal section banners (`State`,
  `Tree flattening`, `Navigation`, `Marking actions`, `Rendering`, `Event loop`, `Saving`).
  Converting it to `src/bin/human_solver/main.rs` + modules along exactly those banner lines
  (`state.rs`, `status.rs`, `actions.rs`, `render.rs`, `persist.rs`) would be a mechanical split
  with real navigation payoff. `handle_key`'s 294 lines are a flat `match` on `KeyCode` where most
  arms are already one-line delegations — the long arms (`'m'`, `'f'`, `'s'`, pickers) can each
  become an `action_*` function like their siblings.
- **`apted/common.rs` (3,901 lines)**: ~1,300 lines are tests. The non-test remainder covers four
  separable concerns: core forest-distance/edit-mapping (`forest_dist`, `compute_edit_mapping`),
  the Myers flat-tree fast path (`myers_lcs` et al., ~200 lines, self-contained), the slot
  repair/promotion heuristics (`SlotCtx` through `repair_leaf_slots`, ~900 lines), and
  `resolve_forest` + entry points. Splitting into `apted/{myers,slots,resolve}.rs` is low-risk
  (all `pub(crate)`) and would take the file under ~1,500 lines. Prior REVIEW.md deferred this
  ("not a problem on its own") — still true, but it keeps growing (was 2,294 lines at that review,
  3,901 now), so the trend argues for doing the split soon.
- `test/optimal_solutions/*.rs` long functions (e.g. `rust_turbopack_module_rule.rs`, 407 lines)
  are ground-truth data tables, not logic — exempt.

---

## 3. Structure & generalization

- **Pipeline pass signature is implicit convention, not a type.** All seven passes implement
  `fn solve(&Code, &Code, &NodeCache, &mut ASTDiff)` and `Diff::from_code` calls them in a
  carefully ordered sequence whose constraints live only in comments (Pass 3 after
  MatchSimilarFlowControl, diagnostics pass between the coarse passes and Pass 3 — both are
  memory-documented "easy to silently re-break" hazards). A minimal
  `trait DiffPass { const NAME: &str; fn solve(…); }` plus an ordered
  `const PIPELINE: &[…]` would (a) let the ordering constraints be asserted in one place or at
  least documented next to a single list, (b) give benchmarking/tracing a hook per pass, and
  (c) force new passes (1.1's metadata refs) through one signature change instead of seven.
- **`optimal_iud.rs` lives under `test/helper/` but is a full algorithm** (the exponential oracle
  used by `benchmark_optimal_solutions` and tests). Placement is defensible (it must never ship in
  the production path), but the name `memoo` for the memoization map and the misspelled
  `unmached`/`first_unmached_node_index` in its public-ish internals hurt grep-ability
  (searching "unmatched" misses them).
- **`NodeCache`'s transmuted `'static` lifetime** (diff.rs:40–54) is thoroughly documented, and
  callers are currently disciplined. If it ever grows another caller, consider the standard
  self-referential escape: make `NodeCache<'tree>` borrow properly and let the few construction
  sites own `Code` first. Documented-unsound-by-convention is the weakest structural point in an
  otherwise safe crate; no action urgent.
- **`ensure_parsed` / metadata-population responsibilities are split across `Code::parse`,
  `Code::ensure_parsed`, `from_string`, `from_file`** with slightly different guarantees (tests
  exist for each combination, which is itself a hint the state machine has too many entry
  states). Not urgent; worth a doc comment on `Code` stating which fields are guaranteed after
  which constructor.

---

## 4. Readability

- **Dead stores flagged by rustc in `engine.rs`** (`unused_assignments`): initializers at
  lines 577 (`current_forest_size2`), 579 (`current_forest_cost2`), 593 (`l_f_last`), 698 (`sp3`)
  are never read before being overwritten. In hand-ported numerical code these are harmless but
  they generate warning noise on every build; deleting the initial values (or restructuring to
  `let … = if …` where trivial) keeps the port shape while silencing the lint. Safe because the
  fuzz/oracle tests in `common.rs` pin behavior.
- **Typos in identifiers and docs** (all trivially fixable, some hurt search):
  `memoo`, `unmached` (optimal_iud.rs, incl. parameter names), "forrest", "theorethically",
  "maping" (diff.rs doc comments), "mapps" (solve_structurally_identical_trees.rs:31), test name
  `hello_world_translations_in_all_langauges` (diff.rs:787), recurring "it's" where "its" is meant
  (ASTMappingOperation docs).
- **Redundant shadow blocks** in both twin passes:
  `{ let after_node_id = matching_after_node.id(); … }` re-derives a value already bound two lines
  up and adds an indentation level to an already 5-deep nest (solve_identical_trees.rs:98,
  solve_structurally_identical_trees.rs:98). Goes away with 1.2.
- **Doc-comment style is split** between `/** … */` block style (older core files: diff.rs,
  helper.rs, optimal_iud.rs) and `///` line style (newer files: human_solver.rs, the newer
  passes). Rustfmt/idiom favors `///`; a mechanical conversion pass would make the codebase read
  uniformly, but do it in a dedicated commit so it doesn't pollute diffs.
- `Diff::from_code`'s pipeline comments are good; the `// TODO: switch to Algorithm::Apted once
  its performance gap vs Zhang-Shasha is resolved` (diff.rs:225) duplicates TODO.md state and can
  drift — pointing at TODO.md instead keeps one source of truth.

---

## 5. Suggested order of attack

1. **1.1 + 1.2 + 1.3 together** — one refactor of the two twin passes plus threading
   `&ASTMetadata` through the pipeline removes the largest duplication, the per-pass deep clones,
   and the O(n²) scans, and fixes the documented duplicate-collapse TODO. Guarded by the
   existing `optimal_solutions` suite + `benchmark_optimal_solutions` (baseline: 270 mismatches).
2. **1.5 + 1.6 + 1.9(blob)** — introduce one shared bin-support module; small, zero-risk,
   makes `commit_stats` deterministic.
3. **1.4** — mechanical, shrinks the unsafe surface to one site.
4. **2: split `human_solver.rs` and `apted/common.rs` along existing seams** — pure moves,
   no behavior change; best done when no parallel human_solver work is in flight (it is actively
   used interactively).
5. **1.7 / 1.8 side-parameterization** — highest-value generalization but touches the trickiest
   code; do after 1 so the test suite is exercising the reorganized passes, and verify with the
   fuzz oracles in `apted/common.rs`.
6. **4: typo/lint sweep** — any time, single mechanical commit.

Not recommended: restructuring `engine.rs`'s APTED internals (`spf_a`, strategy functions) beyond
the lint-silencing in §4 — prior benchmarking discipline applies (profile via
`benches/diff_code_benchmark` first), and the mirrored `_l`/`_r` variants follow the published
algorithm's own structure.
