# Next algorithmic improvements to implement

*  After each top-down heuristic, run a bottom-up heuristic that detects nodes whose children are
   already mapped to each other and then match those nodes too. We can use the ASTMatchinReason
   "BottomUpExpansion". We can also set a threshold, so that the match is valid if X% of nodes
   match. E.g. 90% to start with.
*  Use the values more. At the moment, the node values are used in a all-or-nothing match. But we
   could also use the value similarity to compute the cost, so that identifiers that look more alike
   are cheaper to match in APTED.

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
