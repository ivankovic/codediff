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
//! **One file per fixture, holding every judgement this repository makes about it.**
//!
//! Each file carries up to two tests over the same fixture's two independent ground truths (see
//! `HumanTextMapping`), and neither implies the other - a fixture can map every node exactly right
//! and still paint the wrong bytes:
//!
//!   * `mapping()` - codediff's node mapping against `human_mapping.json`'s `entries`, exact or
//!     clamped to a recorded number of mismatches.
//!   * `mapping_details()` - where a fixture needs specific nodes asserted by hand rather than the
//!     whole mapping compared at once. Ten fixtures have one; it is not a substitute for
//!     `mapping()` and most carry both.
//!   * `painting()` - codediff's *rendering* against the same file's `text_mappings`, clamped to a
//!     percentage of disagreeing bytes, checked under both the `Minimal` and `Full` presets of
//!     [`RenderOptions`](crate::diff::text::RenderOptions). Present only for the fixtures that
//!     have been painted.
//!
//! **The names are fixed, and there are only four.** They used to vary - `optimal_solution`,
//! `matches_human_solution`, and one named after its own fixture - and three files carried two
//! tests asserting byte-for-byte the same thing under two of those names. A test whose name is a
//! matter of taste is a test nobody can find, and a duplicate nobody notices is one more place a
//! change has to be made twice.
//!
//! These were two parallel module trees, `optimal_solutions/` and `painting_agreement/`, with one
//! file per fixture in each. Merging them costs nothing structurally - every painted fixture
//! already had a mapping stub, so there were no orphans - and buys the thing the split was
//! quietly denying: everything anyone has concluded about one fixture is now in one place, rather
//! than split across two files nobody reads together.
//!
//! **The one-file-per-fixture layout is load-bearing and stays.** A clamp accretes an explanation
//! of why codediff and the ground truth differ, and that explanation needs somewhere nothing
//! overwrites. It also makes the rule that a clamp only moves when the measurement no longer fits
//! it *structural* rather than remembered: a fixture whose number did not change is a file nobody
//! rewrites. Regenerating a single shared file twice once loosened a clamp that still held, on
//! nothing but the regenerator's own rounding - which is also why
//! `human_mapping::stub_mapping_limits` reads these files rather than the other way round.
//!
//! **Every painting clamp is large.** Nothing agrees exactly yet; the rates run from hundredths of
//! a percent to about 60%. These are recorded distances, not targets, and the rate is a percentage
//! rather than a count because the fixtures span three orders of magnitude in size - a count would
//! let one large fixture's residual dwarf every small fixture's exactness. Lower one when a change
//! earns it; a rise is a regression.
//!
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
