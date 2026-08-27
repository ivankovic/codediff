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

use crate::test::helper::human_mapping::assert_matches_human_painting_within_limit;

#[test]
fn painting_agreement() -> Result<()> {
    // measured 2026-08-27: minimal 0.000% (0 bytes), full 0.685% (1 of 146 bytes)
    //
    // Minimal is exact and Full is not, which is the expected direction: Full paints the
    // structural punctuation and whitespace Minimal drops, so it has strictly more bytes to be
    // wrong about. The clamp records the worse of the two.
    assert_matches_human_painting_within_limit("rust-hello-world-added-message", 0.69)
}
