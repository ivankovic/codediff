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

#[test]
fn optimal_solution() -> Result<()> {
    // The method body is wrapped in a new `try { ... } catch (...) { ... }`. The method's own
    // top-level `block` keeps the same *role* (the method body) but now contains just the one
    // `try_statement`, while the `try`'s own new inner `block` contains the original 5 statements
    // verbatim - byte-identical to the *before* side's top-level block. codediff's hash matcher
    // (correctly, by content) pairs the before top-level block with the try's inner block (both
    // are the same bytes), instead of the human's structurally-preferred outer-to-outer pairing
    // with the try's inner block as a fresh Insert. This is the documented "container added around
    // moved code" class of gap (see `TODO.md` / prior `GreedyAnchorBlocks`/`final_pass cost gate`
    // investigations) - not attempted again here, since past attempts at a general fix for this
    // pattern were net-negative or reverted.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "java-add-exception-handling",
        6,
        2,
    )
}
