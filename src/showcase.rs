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
//! The data behind the GitHub Pages showcase (`src/bin/generate_showcase.rs`): the browser viewer
//! under `assets/viewer/` running on static files. The viewer asks for JSON over `/api/*`, and the
//! showcase's fetch shim answers from files baked by these types, so the page paints what the TUI
//! would with no server behind it. Built only with `test-fixtures`, which the generator needs anyway.
//!
//! * [`payload`] is a computed diff, with every column converted from this crate's byte offsets
//!   to the UTF-16 code units a browser indexes strings by, plus syntect's highlighting as colour
//!   spans, so the page needs no highlighter of its own.
//! * [`state`] is everything the page needs to draw its first frame: themes, presets, help text.
//!
//! Everything the page does between requests - cursor, scroll, search, dialogs, the overlay
//! painting rules - is `assets/viewer/model.js`, a port of the corresponding
//! `tui::widgets::code_viewer`/`tui::components::diff_viewer` logic, tested under plain Node.

pub mod payload;
pub mod state;
