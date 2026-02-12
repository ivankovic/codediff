/*  This file is part of the CodeDiff code diffing tool.
 *
 *  Copyright (C) 2025 Marko Ivankovic
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
use codediff::diff;

mod common;

#[test]
fn hand_written_tests() {
    let inputs = common::load_hand_written_test_pairs().expect("Failed to load inputs");

    for input in &inputs {
        let _ = diff(&input.before, &input.after);
        // assert_eq!(d.unix_diff_format, input.unix_diff);
        // assert_eq!(d.gumtree_diff_format, input.gumtree_diff);
    }
}
