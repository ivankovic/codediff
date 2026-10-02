/*  This file is part of the OmniDiff code diffing tool.
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

//! Classifies paths into the file-type categories of `code::tip`, the classifier behind the
//! introductory paper's corpus figures, for research scripts that cannot call it: one path per line
//! on stdin, `path<TAB>category` per line on stdout, the category empty when the classifier has none.
//! `analysis/change_census.py` uses it so its census counts in the same categories as the paper.

use std::io::{BufRead, BufWriter, Write};

fn main() -> std::io::Result<()> {
    let stdin = std::io::stdin().lock();
    let mut out = BufWriter::new(std::io::stdout().lock());
    for path in stdin.lines() {
        let path = path?;
        let category = omnidiff::code::tip::type_from_path(std::path::Path::new(&path))
            .map(|tip| format!("{tip:?}"))
            .unwrap_or_default();
        writeln!(out, "{path}\t{category}")?;
    }
    out.flush()
}
