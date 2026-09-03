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
//! **What belongs in a stub comment, and what does not.**
//!
//! A fixture's prose has two homes, and they answer different questions:
//!
//!   * its `description.md`, in the fixture directory, says **what the fixture is and what it
//!     demands** - "Requires an N:M mapping. A rare case of 1:2." That is a fact about the data,
//!     true whoever is diffing it, and it travels with the directory (see `helper::read_note`).
//!   * a stub comment here says **why codediff falls short of that**, and is the only justification
//!     a clamped limit ever gets - "a couple of its string-literal tokens coincidentally match
//!     identical tokens elsewhere in the file". That is a fact about this implementation, and it
//!     stops being true the moment someone fixes it.
//!
//! Keeping them apart is what stops either from being rewritten to say the other's thing. A note
//! about the data does not belong next to a number that a fix will change; an explanation of a
//! residual does not belong in a file that describes the fixture to people who are not reading
//! this code.
//!
//! `the_clamped_stubs_explain_their_limits` enforces the second half: a limit is a claim that
//! codediff cannot currently do better, and a claim with no argument behind it is indistinguishable
//! from a number nobody has revisited - which is exactly what 49 of these turned out to be.

// Mirrors src/test/data/diffs/'s four-way split (see `test::helper::DIFF_DATASETS`): each of
// these is its own mod-list file, one `#[cfg(test)] mod <name>;` per fixture in that dataset.
#[cfg(test)]
mod full;
#[cfg(test)]
mod handmade;
#[cfg(test)]
mod small;
#[cfg(test)]
mod stratified;
