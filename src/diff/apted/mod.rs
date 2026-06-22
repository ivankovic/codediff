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

//! Tree-edit-distance computation, split across three files: `common` (shared
//! infrastructure - cost model, indexing, delta/forest-distance tables, the
//! backtrace that turns a populated delta table into an `ASTDiff`, and the public
//! entry points), `zhang_shasha` (the classic Zhang-Shasha algorithm), and `engine`
//! (the APTED algorithm: `gted`/`spfL`/`spfR`/`spfA` plus optimal-strategy
//! computation).

mod common;
mod engine;
mod zhang_shasha;

pub use common::{for_nodes, for_roots, Algorithm};
