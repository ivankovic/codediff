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
    test::helper::human_mapping::assert_matches_human_mapping("rust-no-change")
}

#[test]
fn painting() -> Result<()> {
    // measured 2026-08-27: minimal 0.000%, full 0.000% - zero disagreeing bytes, both modes.
    //
    // Clamped at 0.0, so this is a real assertion rather than a recorded distance: the first
    // fixture in the corpus where codediff's rendering and the human painting agree byte for
    // byte. A no-change pair is the easiest possible case - every byte is Identical and there is
    // nothing to attribute - so read it as the floor working, not as the metric being solved.
    // If this ever rises, something has broken in the unchanged path, which is worth a hard
    // failure.
    assert_matches_human_painting_within_limit("rust-no-change", 0.0)
}
