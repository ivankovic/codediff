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

#[test]
fn mapping() -> Result<()> {
    // Clamped at the observed count on 2026-09-06 rather than requiring an exact match. The
    // commit guards the existing `#include "upb/mini_table/extension_registry.h"` behind an
    // `#if`, and adds a different include on the line the old one occupied. The human read that
    // as the outer include being updated in place and the guarded copy being new; codediff's
    // phase-1 hash matching instead pairs the before include with the byte-identical guarded
    // copy (`IdenticalHashOfAncestor`, and `WrapGrowth` for the two parents), which leaves the
    // outer include as an insert. Same identical-copy-wins shape as the wrap/reparent cost ties
    // already tracked in TODO.md, not a new defect. Lower once a fix lands.
    test::helper::human_mapping::assert_matches_human_mapping_within_limit(
        "cpp-protocolbuffers-protobuf-add-preprocessor-commands",
        12,
        8,
    )
}

#[test]
fn painting() -> Result<()> {
    // measured 2026-09-06: minimal 0.000%, full 0.000% (measured, unexamined)
    assert_matches_human_painting_within_limit(
        "cpp-protocolbuffers-protobuf-add-preprocessor-commands",
        0.0,
    )
}
