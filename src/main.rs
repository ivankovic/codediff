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
use clap::Parser;
use std::fs;
use std::path::PathBuf;

#[derive(Parser)]
struct Args {
    before: PathBuf,
    after: PathBuf,
}

fn main() -> std::io::Result<()> {
    let args = Args::parse();

    let before = fs::read_to_string(args.before)?;
    let after = fs::read_to_string(args.after)?;

    let d = codediff::diff_strings(&before, &after, &codediff::code::Language::Rust);
    println!("Diff: {:?}", d);

    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn devex_infrastructure_test() {}
}
