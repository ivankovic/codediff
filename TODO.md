# Next algorithmic improvements to implement

*  IMPLEMENTED, threshold tuned, benefit not established (2026-07-11): bottom-up heuristic that
   detects nodes whose descendants are already mapped to each other and matches those nodes too,
   via `ASTMappingReason::BottomUpExpansion` (`src/diff/solve_bottom_up_expansion.rs`), gated by a
   Dice coefficient over full subtrees (a direct-children ratio was tried first and rejected - see
   that file's doc comment). Wired into `Diff::from_code` at a single, deliberately late call site
   (right before Pass 3's orphan blanket-delete/insert) after an earlier "after every top-down
   heuristic" placement regressed 4 `optimal_solutions` fixtures by letting a plausible-but-wrong
   candidate preempt a later, more precise pass.
   `DICE_THRESHOLD` was then swept from 0.5 to 0.95 against `benchmark_optimal_solutions`: 0.8-0.95
   all tie the 742/0 baseline exactly (identical mismatch count on every fixture); 0.78 and below
   start regressing (0.78 -> 746, 0.75 -> 749, 0.5 -> 826), and those regressions are real content
   mismatches (`identifier`/`scoped_identifier`/`field_initializer`, a `statement_block` matched to
   the wrong arrow function) - not generic/punctuation-token ties (`}`, `)`) that would suggest an
   equally-valid alternate optimal solution worth flagging for human review instead of reverting.
   No throughput difference was measurable at any threshold either (one release-build, single-run
   comparison at 0.85 vs. the pass disabled: within noise). Landed at 0.9 - it ties every other safe
   value on outcome while keeping the largest margin from the ~0.79 regression cliff, and there's no
   evidence a lower value buys anything to justify sitting closer to that cliff.
   Same situation as `identical-statement-runs` in the memory log: implemented, correct, and now
   tuned, but whether it's worth keeping in the pipeline at all is still a call for whoever picks
   this up next - it has fired ~29 times across the fixture corpus without net effect on either
   accuracy or measured speed.
*  Use the values more. At the moment, the node values are used in a all-or-nothing match. But we
   could also use the value similarity to compute the cost, so that identifiers that look more alike
   are cheaper to match in APTED.
   TRIED AND REVERTED (2026-07-11), container-dissimilarity-surcharge variant: `UnitCostModel::ren`
   currently charges 0 to match two same-kind *internal* nodes unconditionally, with the real cost
   of reuse-vs-replace left entirely to the children's recursive edit cost. That unconditional 0 has
   a side effect: it always waives exactly the root's own delete+insert cost (COST_DELETE +
   COST_INSERT = 2), so two same-kind containers are always >=2 cheaper to "match" than to replace
   wholesale, no matter how unrelated their content actually is - this looked like the mechanism
   behind the `rust-algorithm-change`/`kotlin-remove-function` gaps below (pure unit-cost prefers
   reuse the human doesn't want).
   Implemented a quantized-tier surcharge on that branch, reusing `leaf_texts_similar`'s character-
   bigram Dice metric (extracted into `nodes::text_similarity`, continuous 0.0-1.0) applied to the
   *whole subtree's* text (available for free - `ASTNodeMetadata.text` is a full-span
   `utf8_text()`, not leaf-only, despite the field doc saying "for leaf nodes"): similarity >= 0.6
   -> 0 (unchanged), >= 0.3 -> 1, below that -> 2 (cancels the subsidy, capped so it never
   *penalizes* matching relative to delete+insert). Capped at 500 chars of subtree text to keep the
   DP's inner loop cheap.
   Result: `kotlin-remove-function` and `rust-algorithm-change` - the two fixtures this was built
   for - moved by exactly 0 mismatches each. Root cause in hindsight: "near-duplicate" is the whole
   problem description for both gaps - two siblings that read as textually *very similar* to a
   human, which is exactly what character-bigram Dice also scores highly (>= 0.6), so the surcharge
   never engages for the case it targets. Text similarity cannot distinguish "same entity, edited"
   from "different-but-near-identical entity" - by construction they look the same to that metric,
   so no threshold value here can ever separate them.
   Meanwhile it broke 5 *previously-perfect* (0-mismatch) fixtures - `rust-data-structure` (0->9),
   `kotlin-refactor-function` (0->5), `python-refactoring` (0->5), `kotlin-add-data-class` (0->2),
   `javascript-refactor-arrow-func` (0->1) - plus regressed 2 already-imperfect ones
   (`typescript-async-await` +10, `cpp-ladybird-refactor-variables-if-changes` +6). Checked via
   `--details` per the standing "check for punctuation-tie false-regressions before reverting"
   policy: all real content mismatches (`identifier`, `token_tree`, `user_type`) - not `}`/`)` ties.
   Root cause: small containers with several internally-differing identifiers (e.g. a macro
   `token_tree`, a struct's field list) have low *aggregate* text similarity even when they're the
   correct match - the surcharge punished exactly the kind of legitimate reuse the existing
   per-child recursive cost was already handling correctly. TOTAL mismatches did drop 742 -> 739,
   but that's 4 already-broken fixtures improving by more than 5 clean ones broke - not a trade
   worth taking, especially against the target case's 0/0 result.
   Reverted in full (`UnitCostModel::ren`'s internal-node branch, `nodes::text_similarity`
   extraction). Whoever picks up "use the values more" next: subtree-text similarity is the wrong
   signal for the reuse-vs-replace question specifically because it's blind to exactly the
   distinction that matters (similar-looking-but-distinct vs. actually-the-same-thing-edited) -
   this needs either a different signal entirely (e.g. positional/identity context: is there a
   *closer* candidate elsewhere that a plain nearest-text-match would find instead?) or accepting
   this class of gap per the three options already listed under `rust-algorithm-change` below. The
   leaf-level idea (graduate `COST_UPDATE` itself by identifier similarity, rather than internal-
   node `ren`) is untested and may still be worth trying - it wasn't what this attempt built, and
   doesn't have the same "can't distinguish near-duplicate from renamed" problem since a leaf
   rename *is* exactly the "same entity, edited" case by construction.

# Next features to implement

*  Add "Diff script" generation that can take the ASTDiff and make a "insert, move, update, delete"
   script out of it.

# TUI follow-ups

*  Mouse support and bracketed paste handling in the TUI.
*  Re-review TUI suspend/resume (Ctrl-Z) behavior, not touched since the async event loop rewrite.
*  Headless mode (`--headless`) is still unimplemented.
*  Revisit the `Update` diff color (currently magenta) once seen against more real diffs.

# Possible code health improvements

*  Make code.rs parse code in the from_string if possible, and then remove parsing from diff_code
   diff.rs

# Diff algorithm accuracy (optimal_solutions gaps)

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
