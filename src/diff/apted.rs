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

//! Tree-edit-distance computation. `common` holds the cost model, indexing, the residual-forest
//! fallback, the containment-pruned slot bookkeeping, the Myers flat-tree path and the
//! pre-matching passes; `engine` is APTED; `zhang_shasha` is the classic algorithm, kept as the
//! test oracle.

mod common;
mod engine;
// Test oracle for the fuzz tests in `common/tests.rs`.
#[cfg(test)]
mod zhang_shasha;

pub use common::{Algorithm, for_nodes, for_roots};
pub(crate) use common::{
    myers_lcs, prematch_identical_statement_siblings, prematch_unique_named_locals,
};

use crate::code::Code;
use crate::diff::ASTDiff;

/// The phase-6 terminal fallback: a Myers-LCS alignment of the still-unmatched residual forest,
/// cheaper than [`for_roots`]'s whole-tree APTED. A no-op when either side has no AST.
pub fn for_roots_fallback(before: &Code, after: &Code, source: &'static str, diff: &mut ASTDiff) {
    // `ast: None` is a valid state (no grammar for the language), not a bug.
    if before.ast.is_none() || after.ast.is_none() {
        return;
    }

    let before_metadata = crate::code::metadata::metadata_of(before);
    let after_metadata = crate::code::metadata::metadata_of(after);

    let before_root_id = before.ast.as_ref().unwrap().root_node().id();
    let after_root_id = after.ast.as_ref().unwrap().root_node().id();

    common::resolve_residual_forest_via_myers_lcs(
        &before_metadata,
        &after_metadata,
        before_root_id,
        after_root_id,
        source,
        diff,
    );
}
