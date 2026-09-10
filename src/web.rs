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
//! The browser front end: `codediff-web` (`src/web_main.rs`) serves the page under `assets/web/`
//! from a local HTTP server, and the page drives the same diff the TUI draws through a small JSON
//! API. Feature-gated (`web`), off by default. See `SPECS.md` in this directory for the design and
//! its decision log.
//!
//! Division of labour, mirroring the TUI's own split between `app.rs` and its components:
//!
//! * [`session`] is the controller - what the TUI keeps in `App` and `DiffViewer` between
//!   keystrokes that is *not* view state: the open pair, the persisted settings, the last computed
//!   diff (unfiltered, so a render-options change re-filters without re-diffing). Every method is
//!   synchronous and touches no socket, so it is unit-tested the way `App` is.
//! * [`payload`] is what crosses the wire: the diff with every column already converted from the
//!   byte offsets this crate uses everywhere (see `diff::text_range`) to the UTF-16 code units a
//!   browser indexes strings by, plus syntect's highlighting as colour spans, so the page needs no
//!   highlighter of its own.
//! * [`server`] routes requests to the session and enforces the two checks that keep a page
//!   served on localhost from being driven by some other page the user has open.
//! * [`http`] is the HTTP/1.1 subset underneath - deliberately not a server crate, see its own
//!   module comment.
//!
//! Everything the browser does *between* requests - cursor, scroll, focus, search, dialogs, the
//! change-navigation order, the overlay painting rules - is `assets/web/model.js`, a port of the
//! corresponding `tui::widgets::code_viewer`/`tui::components::diff_viewer` logic, tested under
//! plain Node the way `assets/mapping_site/` is.

pub mod http;
pub mod payload;
pub mod server;
pub mod session;
