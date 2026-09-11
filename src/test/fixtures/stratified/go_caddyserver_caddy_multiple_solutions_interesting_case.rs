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
    // measured 2026-09-11: 3 mismatch(es), 3 visible. The fixture's own name records the reason:
    // several genuinely different mappings are defensible here depending on whether given node
    // kinds are preferred to match, and codediff takes a different one than the painter did.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "go-caddyserver-caddy-multiple-solutions-interesting-case",
        3,
        3,
    )
}

#[test]
fn painting() -> Result<()> {
    // measured 2026-09-11: minimal 0.014%, full 0.015%
    assert_matches_human_painting_within_limit(
        "go-caddyserver-caddy-multiple-solutions-interesting-case",
        0.03,
    )
}

#[test]
fn invariants() -> Result<()> {
    assert_ground_truth_invariants("go-caddyserver-caddy-multiple-solutions-interesting-case")
}
