/*  This file is part of the CodeDiff code diffing tool.
 *
 *  Copyright (C) 2026 Marko Ivankovic
 *
 *  This program is free software: you can redistribute it and/or modify
 *  it under the terms of the GNU Affero General Public License as published
 *  by the Free Software Foundation, either version 3 of the License, or
 *  (at your option) any later version.
 *
 *  This program is distributed in the hope that it will be useful,
 *  but WITHOUT ANY WARRANTY; without even the implied warranty of
 *  MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
 *  GNU Affero General Public License for more details.
 *
 *  You should have received a copy of the GNU Affero General Public License
 *  along with this program. If not, see <https://www.gnu.org/licenses/>.
 */
use anyhow::Result;

use crate::test;

// This test case's core is:
//
// total = 0
//
// changing to
//
// total = sum(numbers)
//
// This is the core of this diff, where AST diffing performs much better than text based
// diff. nvim -d detect this as a delete and then an update of the for loop, which is
// perhaps reasonable for text based diffing, but is obviously suboptimal. A much better
// diff that more closely follows the logic of code and what the human has actually done is
// to say that the for loop was deleted, and that the assignment was changed.
//
// Here's how the AST looks like if we focus on the assignment:
//
// Before:
//
// assignment
//   |- identifier
//   |- =
//   |- integer
//
// After:
//
// assignment
//   |- identifier
//   |- =
//   |- call
//       |- identifier
//       |- argument_list
//            |- (
//            |- identifier
//            |- )
//
//  With the AST visible, it's clear that the optimal solution is that the identifier and
//  equals signs are an Identical match, the integer is a delete and the call with it's
//  subtree is an add.
#[test]
fn matches_human_solution() -> Result<()> {
    // 2026-08-05: `prematch_identical_statement_siblings` (`apted::common`) briefly regressed this
    // fixture. It pre-matches this function's byte-identical `average = total / count` statement,
    // excising it (and its `total`/`count` identifier tokens) from the forest APTED sees before
    // real APTED runs. That exposed a real gap in `ContainmentCtx`: it only forbade a pairing that
    // contradicted where a pruned *descendant* landed relative to a hollowed-out *ancestor*, not
    // one that silently reordered past an unrelated pruned *sibling* (there's no ancestor-
    // descendant relationship between `average` and `total = 0` to catch). Without that guard,
    // `total = 0`'s `total` could match some unrelated `total` occurrence positioned after
    // `average`'s counterpart, losing the rename-target pairing needed to resolve `total = 0` ->
    // `total = sum(numbers)` (the fixture's documented optimal solution) and falling back to a
    // wholesale statement delete instead.
    //
    // Fixed at the root by extending `ContainmentCtx::adjust` with a sibling-order-consistency
    // check: every pruned chunk's root position (`preorder_index`) is recorded, and a candidate
    // pairing is only allowed if both nodes have the same count of pruned anchors preceding them
    // on their respective side - see `ContainmentCtx`'s doc comment in `apted/common.rs`.
    test::helper::human_mapping::assert_matches_human_mapping("python-refactoring")
}
