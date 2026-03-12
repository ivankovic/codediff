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
use std::collections::HashMap;

/**
* Compute a hash of the given TreeSitter tree from the given root node.
* The result is a pair of hash maps:
*   - One hash map going from the TS Node IDs to their hashes
*   - One going from hashes to their TS Node IDs.
*
* Note that TS Node IDs are semi-stable. The TS documentation goes into detail, but for our purpose
* they are stable between edits and re-parsing, and since we do neither we are ok.
*
* The aim is for the hash to have the following properties:
*   - Fast. Speed is of the essence. 99.999% of files in the full dataset should hash in under 50ms.
*   - Robust. The hash is used for duplicate detection so statistical properties must be robust.
*
* There is NO requirement for security. Crypto hashes are way too slow for our use case and
* reversing the hash is irrelevant, we return the reverse map anyhow.
*/
pub fn hash_treesitter_tree() -> Result<HashMap<usize, u64>> {}
