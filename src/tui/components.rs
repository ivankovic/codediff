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
pub mod code_viewer;
pub mod diff_mode_dialog;
pub mod diff_viewer;
pub mod file_dialog;
pub mod help_modal;
pub mod search_modal;
pub mod theme_dialog;

use anyhow::Result;
use crossterm::event::{KeyEvent, MouseEvent};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    prelude::Stylize,
    style::{Color, Style},
    symbols::border,
    text::Line,
    widgets::{Block, Borders, List, ListItem, ListState},
};
use tokio::sync::mpsc::UnboundedSender;

use crate::tui::actions::Action;
use crate::tui::events::Event;

/// Moves a list-dialog selection index up (`delta < 0`) or down (`delta > 0`) by one, clamped to
/// `[0, len)`. Shared Up/Down handler behind `ThemeDialog` and `FileDialog`.
pub fn move_selection(selected: &mut usize, delta: i32, len: usize) {
    if delta < 0 {
        *selected = selected.saturating_sub(1);
    } else if *selected + 1 < len {
        *selected += 1;
    }
}

/// Renders a bordered, cyan-highlighted, single-column list popup with a dim hint line below it:
/// the shared visual scaffold behind `ThemeDialog` and `FileDialog`'s `draw`, which differ only in
/// their title and how each row is labeled.
pub fn render_list_dialog(
    frame: &mut Frame,
    area: Rect,
    title: Line<'static>,
    items: Vec<ListItem<'static>>,
    selected: usize,
    hint: &'static str,
) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(area);

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_set(border::ROUNDED)
        .border_style(Style::new().fg(Color::Cyan));

    let mut list_state = ListState::default().with_selected(Some(selected));
    let list = List::new(items)
        .block(block)
        .highlight_style(Style::new().bg(Color::Blue).bold());

    frame.render_stateful_widget(list, layout[0], &mut list_state);
    frame.render_widget(Line::from(hint).dim(), layout[1]);
}

/// A visual and interactive element of the TUI.
pub trait Component {
    /// Register an action handler that can send actions for processing if necessary.
    ///
    /// # Arguments
    ///
    /// * `tx` - An unbounded sender that can send actions.
    ///
    /// # Returns
    ///
    /// * [`Result<()>`] - An Ok result or an error.
    fn register_action_handler(&mut self, _tx: UnboundedSender<Action>) -> Result<()> {
        Ok(())
    }

    /// Initialize the component with a specified area if necessary.
    ///
    /// # Arguments
    ///
    /// * `area` - Rectangular area to initialize the component within.
    ///
    /// # Returns
    ///
    /// * [`Result<()>`] - An Ok result or an error.
    fn init(&mut self, _area: Rect) -> Result<()> {
        Ok(())
    }

    /// Handle an incoming terminal event and produce an action if necessary.
    ///
    /// # Arguments
    ///
    /// * `event` - The event to be processed, or `None` if the caller has nothing for this
    ///   component this tick.
    ///
    /// # Returns
    ///
    /// * [`Result<Option<Action>>`] - An action to be processed or none.
    fn handle_events(&mut self, event: Option<Event>) -> Result<Option<Action>> {
        let Some(event) = event else {
            return Ok(None);
        };
        let action = match event {
            Event::Key(key_event) => self.handle_key_event(key_event)?,
            Event::Mouse(mouse_event) => self.handle_mouse_event(mouse_event)?,
            _ => None,
        };
        Ok(action)
    }

    /// Handle key events and produce actions if necessary.
    ///
    /// # Arguments
    ///
    /// * `key` - A key event to be processed.
    ///
    /// # Returns
    ///
    /// * [`Result<Option<Action>>`] - An action to be processed or none.
    fn handle_key_event(&mut self, _key: KeyEvent) -> Result<Option<Action>> {
        Ok(None)
    }

    /// Handle mouse events and produce actions if necessary.
    ///
    /// # Arguments
    ///
    /// * `mouse` - A mouse event to be processed.
    ///
    /// # Returns
    ///
    /// * [`Result<Option<Action>>`] - An action to be processed or none.
    fn handle_mouse_event(&mut self, _mouse: MouseEvent) -> Result<Option<Action>> {
        Ok(None)
    }

    /// Update the state of the component based on a received action. (REQUIRED)
    ///
    /// # Arguments
    ///
    /// * `action` - An action that may modify the state of the component.
    ///
    /// # Returns
    ///
    /// * [`Result<Option<Action>>`] - An action to be processed or none.
    fn update(&mut self, _action: Action) -> Result<Option<Action>> {
        Ok(None)
    }

    /// Render the component on the screen. (REQUIRED)
    ///
    /// # Arguments
    ///
    /// * `f` - A frame used for rendering.
    /// * `area` - The area in which the component should be drawn.
    ///
    /// # Returns
    ///
    /// * [`Result<()>`] - An Ok result or an error.
    fn draw(&mut self, frame: &mut Frame, area: Rect) -> Result<()>;
}
