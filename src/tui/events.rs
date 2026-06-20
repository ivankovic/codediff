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
use crossterm::event::{KeyEvent, MouseEvent};

/// An event coming out of `UI::next_event`'s merged tick/render/input select loop.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// The tick timer fired (app logic update, independent of rendering).
    Tick,
    /// The render timer fired; the app should redraw.
    Render,
    /// The terminal was resized to (width, height).
    Resize(u16, u16),
    /// A key was pressed.
    Key(KeyEvent),
    /// A mouse event occurred.
    Mouse(MouseEvent),
}
