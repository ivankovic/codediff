# Next features to implement

*  cpp-fix-segfault shows a problem with the current algorithm. The node KIND < transforms to <=.
   But even though this is a node KIND, not value, change, we still want to allow this. At the
   moment, we don't, but we probably should. At the same time, we probably don't want to allow all
   possible kind updates, e.g. for loop to function declaration is probably not ok... This needs
   brainstorming.
*  Add "Diff script" generation that can take the ASTDiff and make a "insert, move, update, delete"
   script out of it.

# TUI follow-ups

*  Mouse support and bracketed paste handling in the TUI.
*  Re-review TUI suspend/resume (Ctrl-Z) behavior, not touched since the async event loop rewrite.
*  Headless mode (`--headless`) is still unimplemented.
*  Revisit the `Update` diff color (currently magenta) once seen against more real diffs.

# Benchmarking

*  Add diff-cost benchmarking to diff.rs. The diff cost is the number of inserts + deletions + updates.

# Possible code health improvements

*  Make code.rs parse code in the from_string if possible, and then remove parsing from diff_code
   diff.rs
*  Make all handmade test diff directories in src/test/data/diffs start with the language name, e.g.
   not "hello-world-added-message" but "rust-hello-world-added-message" etc.

# Diff algorithm accuracy (optimal_solutions gaps)

## Top 5 quality ideas, ranked by expected impact (benchmark 2026-07-06: 270 total mismatches)

**STATUS (implemented 2026-07-06, same day): total 270 -> 188.** All five ideas landed, in
modified forms - see HANDOVER.md's ideas for the original designs and the code for what survived
contact with the ground truths:

- Idea 5 (leaf text-similarity + context gating): shipped; grew a two-sided depth-budget context
  check (`update_context_supported`) after one-sided climbs proved either too tight or too loose.
- Idea 3 (slot alignment): shipped as three mechanisms in `improve_slot_alignment`
  (src/diff/apted/common.rs): `validate_fresh_matches` (see idea-4 note), `pull_up_wrapped_matches`
  (wrapper-tie retargeting), `promote_same_slot_pairs` (anchored weighted-LCS promotion +
  `repair_leaf_slots`).
- Idea 2 (move recovery): shipped as `solve_moved_subtrees` (final pipeline pass) with a
  container-identity guard (outermost deleted/inserted reference-ancestor kinds must agree) that
  reconciles turbopack's "moves are matches" ground truth with kotlin-refactor-function's "new
  construct is new code".
- Idea 1 (wrapper detection): fell out of ideas 3+2 combined, as HANDOVER predicted; no separate
  mechanism was needed.
- Idea 4 (container validation): subsumed by `validate_fresh_matches` - a *structural* island rule
  (a fresh match whose both parents are unmatched needs a nearby matched ancestor or
  identical-hash-with-content) instead of the planned recall threshold. No tunable ratio survived.

Remaining failures are: rust-turbopack-module-rule (166; the Pass-3 orphaning gap below is now
the only big lever left), rust-algorithm-change (17; its human_mapping blessed DP artifacts like
`nums.len()` matching `HashSet::new()` - needs a human_solver re-run to decide the intended
philosophy), javascript-fix-promises (2; ground truth maps `{` to the outer wrapper block but `}`
to the inner one - asymmetric brace semantics worth a re-look), kotlin-refactor-function (2;
dissimilar-identifier Update in a matched slot - contradicts js-add-event-listener's ground truth
which blesses the same shape), javascript-add-destructuring (1; before-leaf unmatched while its
slot partner is matched elsewhere - a shape neither promotion nor repair covers yet).

Baseline measured with `cargo run --release --bin benchmark_optimal_solutions`. Worst fixtures:
rust-turbopack-module-rule (207), cpp-optimize-algorithm (26), javascript-add-array-method (14),
python-api-change (10), javascript-fix-promises (5), kotlin-add-data-class (4).

1. **Wrapper insert/unwrap detection.** The single most common human edit the algorithm mangles:
   existing code gets wrapped in a new node (statement wrapped in `try`, expression wrapped in a
   call like `Some(...)` / `Person(...)`, identifier wrapped in `reference_declarator`), or the
   reverse. `ASTMappingOperation::Insert` is *documented* as "inserted between a parent and a
   consecutive subsequence of the parent's children", but the pipeline never actually produces
   that shape - the wrapped content round-trips through delete+insert instead. Detect "hollow"
   inserts/deletes at emission time: an unmatched after-node whose descendants are mostly matched
   content should cost 1 (just the wrapper), keeping the inner matches. Evidence: this exact shape
   appears in rust-turbopack-module-rule (match-arm bodies wrapped in call arguments),
   javascript-fix-promises (`resolve(...)` wrapped in `try`), kotlin-add-data-class (`mapOf`
   arguments re-wrapped in a constructor call). Likely the biggest single lever on the total.

2. **Move-detection recovery pass over unmatched islands.** After all passes, scan
   delete-side/insert-side islands for subtree pairs with identical full hashes (or structural
   hashes above a distinctiveness/size threshold - reuse the `reference_nodes_ordered`-style
   largest-first ordering) and convert them to matches/moves. This is GumTree's "recovery
   mappings" phase, and directly attacks the known moved-code gap: most of
   rust-turbopack-module-rule's 207 mismatches are content that moved into new wrappers and
   currently round-trips as delete+insert. Pairs naturally with idea 1 (moved code usually lands
   inside newly-inserted wrappers).

3. **Slot-aware same-kind container matching.** Humans read "the last statement of this function
   before vs after" as *the same statement, edited* even when its contents changed wholesale
   (cpp-optimize-algorithm: `return min;` -> `return *std::min_element(...);` is
   MatchButNotIdentical in the human ground truth). Conversely they reject reuse of a same-kind
   container from an unrelated slot. Generalize the generic-token small-context gate
   (`emit_match`'s `has_nearby_matched_ancestor`) into a positive signal too: when two same-kind
   containers sit in corresponding slots (matched parent + same sibling role/position), prefer
   matching them; when they don't, require matched-descendant evidence. Targets the remaining
   cpp-optimize-algorithm (26) and javascript-add-array-method (14) mismatches.

4. **Asymmetric (recall-based) container-support validation.** A previous experiment (2026-07-06,
   reverted - `demote_unanchored_matches`, see conversation-era git stash/history) tried GumTree's
   symmetric dice coefficient to demote weakly-supported container matches and made results
   *worse* (47/8 passing tests -> 27/28), because symmetric dice punishes pure-addition edits:
   appending new code under an unchanged container dilutes the ratio even though the match is
   obviously right. The variant worth trying instead: validate with recall of the *smaller* side
   (max of matched/before_size and matched/after_size), so pure additions score ~1.0, and only
   demote containers where both recalls are low - i.e. genuine skeleton reuse across unrelated
   content. Do not retry plain symmetric dice; that experiment already ran and lost.

5. **Text-similarity gating for leaf Updates.** `UnitCostModel::ren` prices any same-kind leaf
   pair at COST_UPDATE regardless of text, so identifier `i` can "Update" into `numbers` across
   unrelated code purely because 1 < 2 (delete+insert). GumTree gates leaf matches on string
   similarity; do the same at emission (the same override point as the generic-token gate): allow
   a leaf Update only when the texts are actually similar (n-gram or normalized edit distance past
   a threshold) or the small context is matched (reusing `has_nearby_matched_ancestor`). Cheap to
   implement, cleans up the scattered single-node mismatches in python-api-change and the
   javascript fixtures.

## Known gaps with full analysis

*  Premature/irreversible pruning in `solve_semantically_structural_nodes`'s Pass 3: when a
   name-keyed anchor (`impl_item`/`function_item` matched by identifier) fails to find a
   counterpart, Pass 3 immediately marks the whole subtree deleted/inserted via an O(n) shortcut,
   before the final full-tree APTED pass (which runs afterward and might find a real
   correspondence) ever gets a chance - `for_roots`/`resolve_forest` filters out anything already
   in `before_node_map`/`after_node_map`, so the decision is irrevocable. Surfaced by the
   `rust-turbopack-module-rule` optimal_solutions test: `impl ModuleType` was renamed to
   `impl ConfiguredModuleType` and its `from_str_with_defaults` method split into `parse` +
   `into_effect`, which share a near-identical parameter list with the original - but since the
   type name changed, they're never even considered for matching, and the whole impl body (234
   node mismatches worth) round-trips through delete+insert instead. Candidate directions: make
   the orphan handling advisory (run it after the full-tree pass, or only fall back to the O(n)
   shortcut if that pass doesn't resolve the node), or add similarity-based anchoring (parameter-
   list shape / identifier Jaccard similarity) as a fallback when name-based anchoring fails.

*  `rust-algorithm-change` (optimal_solutions test): the human-authored ground truth matches
   before's OUTER `for` loop to after's (only) `for` loop, deleting the whole INNER (nested) loop
   and inserting after's `if`/`return`/`seen.insert` body as new. codediff instead matches the
   INNER loop to after's loop and reuses its `if`/`return` body, because that body is a much
   closer syntactic match to after's `if`/`return` body than the outer loop's body is (which wraps
   an entire second `for` loop) - reuse is cheaper than delete+insert under unit cost, so this
   isn't a coin-flip the DP happened to lose.
   Checked this isn't a reachable-but-mistied case: summed the edit cost implied by the complete
   human mapping in `human_mapping.json` (each `delete`/`insert`/`update` entry = 1,
   `insert_with_children` = its subtree size) against codediff's actual root mapping cost from the
   same pipeline - human-implied cost is **96**, codediff's is **10**. That's not a tie needing a
   tiebreak; the human's reading is ~10x more edit operations because it requires recognizing that
   the whole loop got algorithmically replaced (brute-force nested loop -> HashSet single loop) and
   deliberately *not* reusing the syntactically-similar `if`/`return` shape. Pure syntactic
   tree-edit-distance has no signal for that - it only ever minimizes edit operations, so it will
   always prefer reuse when reuse is available and cheaper, regardless of whether the reused code
   is semantically related. Not fixable without changing the objective itself (favor "replace this
   whole container wholesale" over minimizing token-level edit script size in some cases), which is
   a deliberate design tradeoff to weigh - not a bug, and not something a local cost-model tweak or
   DP tie-break can produce. Three ways to actually move on this, for whoever picks it up:
   (a) accept as a known limitation of syntax-only diffing and leave the test un-green or delete it,
   (b) pursue an explicit "prefer replacing a whole matched container over token reuse past some
   depth/size" heuristic as its own effort (same risk class as the reverted hash-based
   pre-matching pass mentioned in `resolve_forest` - arbitrary interior-node bias has bitten this
   codebase before, so it needs its own careful validation against the full optimal_solutions
   suite, not just this one case), or (c) reconsider whether this particular hand-authored ground
   truth is asking for algorithmic/semantic understanding that's out of scope for an AST-structural
   differ.
