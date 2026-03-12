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

use crate::code::Code;

/**
* Compute a hash of the given TreeSitter tree from the given root node.
*
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
pub fn hash_code(code: &Code) -> Result<(HashMap<usize, u64>, HashMap<u64, usize>)> {
    let mut node_to_hash = HashMap::new();
    let mut hash_to_node = HashMap::new();

    return Ok((node_to_hash, hash_to_node));
}

#[cfg(test)]
mod tests {
    use anyhow::Result;

    use crate::test::helper;

    use super::*;

    #[test]
    fn hash_empty_rust_code() -> Result<()> {
        let codes = helper::handmade_test_code()?;

        let hello_world = codes
            .get("hello-world.rs")
            .expect("hello-world.rs should exist in test data");

        let (node_to_hash, hash_to_node) = hash_code(hello_world)?;

        assert!(!node_to_hash.is_empty());
        assert_eq!(node_to_hash.len(), hash_to_node.len());

        // Note that since the Map is not a multi map, if the two maps have exactly the same size
        // and each element from one map has a matching element in the other map, we don't need to
        // check the other map because it must also be completely covered.
        //
        // Otherwise, either the length would be different or the map would need to contain
        // duplicate keys, i.e. be a multi-map.
        for (node, hash) in node_to_hash {
            let t = hash_to_node.get(&hash);

            match t {
                Some(node_from_hash) => {
                    assert_eq!(&node, node_from_hash);
                }
                None => panic!("Node->Hash map has an entry that doesn't exist in Hash->Node map!"),
            }
        }

        Ok(())
    }
}
