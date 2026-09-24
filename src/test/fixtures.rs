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
//! A file carries at most four tests, under fixed names, over the fixture's two independent ground
//! truths (see `HumanTextMapping`) - a fixture can map every node right and still paint the wrong
//! bytes:
//!
//!   * `mapping()` - the node mapping against `human_mapping.json`'s `entries`, exact or clamped to
//!     a recorded number of mismatches.
//!   * `mapping_details()` - specific nodes asserted by hand; accompanies `mapping()`, never
//!     replaces it.
//!   * `painting()` - the rendering against `text_mappings`, clamped to a percentage of disagreeing
//!     bytes under both the `Minimal` and `Full` presets of
//!     [`RenderOptions`](crate::diff::text::RenderOptions). Only for painted fixtures.
//!   * `invariants()` - the ground truth checked against itself.
//!
//! One file per fixture is load-bearing: a clamp's explanation needs a home nothing regenerates,
//! and a clamp only moves when someone edits its file. A shared file regenerated wholesale loosens
//! clamps that still hold, which is why `human_mapping::stub_mapping_limits` reads these files
//! rather than writing them. A painting clamp is a percentage because fixture sizes span three
//! orders of magnitude; lower it when a change earns it, and treat a rise as a regression.
//!
//! **What a stub comment says.** The fixture's `description.md` says what the fixture *demands*
//! (a fact about the data). A stub comment says why codediff *falls short* of it (a fact about this
//! implementation, false once fixed), and is the only justification a clamp gets.
//! `the_clamped_stubs_explain_their_limits` enforces that every clamped `mapping()` has one.

// One `#[cfg(test)] mod <name>;` per fixture, per dataset (see `test::helper::DIFF_DATASETS`).
#[cfg(test)]
mod defects4j;
#[cfg(test)]
mod full;
#[cfg(test)]
mod handmade;
#[cfg(test)]
mod small;
#[cfg(test)]
mod stratified;
