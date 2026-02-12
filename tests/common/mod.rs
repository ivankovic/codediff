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
 *  MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 *  GNU Affero General Public License for more details.
 *
 *  You should have received a copy of the GNU Affero General Public License
 *  along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */
use anyhow::Result;
use std::fs;
use std::path::PathBuf;

pub struct TestPair {
    pub before_path: PathBuf,
    pub after_path: PathBuf,
    pub before: String,
    pub after: String,
    pub unix_diff: String,
    pub gumtree_diff: String,
}

pub fn load_hand_written_test_pairs() -> Result<Vec<TestPair>> {
    let mut result = Vec::new();

    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("data")
        .join("hand-made");

    println!("Reading hand-made inputs from {:?}", root.as_path());

    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            let before_path = path.join("before.rs");
            let after_path = path.join("after.rs");
            let unix_diff_path = path.join("diff.patch");
            let gumtree_diff_path = path.join("gumtreediff.txt");

            let before = fs::read_to_string(&before_path)?;
            let after = fs::read_to_string(&after_path)?;
            let unix_diff = fs::read_to_string(&unix_diff_path)?;
            let gumtree_diff = fs::read_to_string(&gumtree_diff_path)?;

            result.push(TestPair {
                before_path,
                after_path,
                before,
                after,
                unix_diff,
                gumtree_diff,
            });
        }
    }

    Ok(result)
}
