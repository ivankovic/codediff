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
use crate::test;
use crate::test::helper::human_mapping::assert_matches_human_painting_within_limit;
use anyhow::Result;

#[test]
fn mapping() -> Result<()> {
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

#[test]
fn painting() -> Result<()> {
    // measured 2026-08-26: minimal 1.741%, full 2.611%
    assert_matches_human_painting_within_limit("python-refactoring", 2.63)
}
