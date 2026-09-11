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
use crate::test::helper::human_mapping::assert_matches_human_painting_within_limit;
use crate::test::helper::human_mapping::invariants::assert_ground_truth_invariants;

#[test]
fn mapping() -> Result<()> {
    test::helper::human_mapping::assert_matches_human_mapping(
        "php-nextcloud-server-real-small-change",
    )
}

#[test]
fn painting() -> Result<()> {
    // measured 2026-09-09: minimal 4.824%, full 0.981% (measured, unexamined)
    assert_matches_human_painting_within_limit("php-nextcloud-server-real-small-change", 4.84)
}

#[test]
fn invariants() -> Result<()> {
    // Was pinned at 2 until 2026-09-12: two `Full` rows - the `@var IClientService` docblock line
    // and the `private $clientService;` beside it - painted every visible character `Delete` but
    // left their one leading tab unpainted, which invariant 4 reads as a line changed in whole but
    // painted in part. The note left the call to the author, and the author made it: both tabs are
    // now painted, matching the deleted lines around them, so the fixture is at 0.
    assert_ground_truth_invariants("php-nextcloud-server-real-small-change")
}
