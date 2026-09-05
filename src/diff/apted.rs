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

//! Tree-edit-distance computation. `common` holds the shared infrastructure - the cost model
//! and indexing (`common.rs`), the residual-forest fallback (`common/residual.rs`), the
//! `resolve_forest` entry point and backtrace (`common/resolve.rs`), the containment-pruned
//! slot bookkeeping (`common/slots.rs`), the Myers flat-tree path (`common/myers.rs`) and the
//! pre-matching of uniquely named locals (`common/prematch.rs`); `engine` is the APTED
//! algorithm (`gted`/`spfL`/`spfR`/`spfA` plus optimal-strategy computation); `zhang_shasha`
//! is the classic algorithm, kept as the test oracle.

mod common;
mod engine;
// Test oracle only: the fuzz tests in `common/tests.rs` compare the APTED engine against it.
#[cfg(test)]
mod zhang_shasha;

pub use common::{Algorithm, for_nodes, for_roots};
pub(crate) use common::{
    myers_lcs, prematch_identical_statement_siblings, prematch_unique_named_locals,
};

use crate::code::Code;
use crate::diff::ASTDiff;

/// The phase-6 terminal fallback, cheaper than [`for_roots`]'s whole-tree APTED and run
/// unconditionally: a Myers-LCS alignment of the still-unmatched residual
/// forest instead of full tree-edit-distance. See `common::resolve_residual_forest_via_myers_lcs`
/// for how the residual is collected and aligned.
pub fn for_roots_fallback(before: &Code, after: &Code, source: &'static str, diff: &mut ASTDiff) {
    // See the identical guard/rationale on `for_roots` in `common.rs`.
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
