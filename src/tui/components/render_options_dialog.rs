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
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{Frame, layout::Rect, text::Line, widgets::ListItem};

use super::{Component, move_selection, render_list_dialog};
use crate::diff::text::RenderOptions;
use crate::tui::actions::Action;

/// A hint line explaining every key this dialog answers to, drawn under the option list by
/// `render_list_dialog` - the same scaffold `FileDialog` uses.
const HINT: &str = "↑/↓ move  Space toggle  Enter apply  Esc cancel  1/2 presets";

/// The `M` key's settings panel: one checkbox row per [`RenderOptions`] field, plus two preset
/// shortcuts.
///
/// Every toggle applies and persists immediately, so the diff behind the panel shows what the
/// setting does while it is still open. That is what makes `Esc` restore the options the panel was
/// opened with rather than merely closing it (see [`Self::initial`]): without it there is no way
/// back from a mistaken keystroke, because the mistake is already on disk.
///
/// **`Enter` accepts and closes, `Esc` reverts and closes** - the same split `ThemeDialog` makes,
/// which is the app's only other dialog that writes to the viewer while it is open. Until
/// 2026-09-18 `Enter` was a second toggle key beside `Space` and nothing accepted, so the only
/// ways out of the panel were `Esc`, which undid the visit, and `q`, which quit the application
/// (it is handled globally, before the event reaches any dialog). `Space` remains the toggle.
///
/// The presets are on `1`/`2` rather than `m`/`f` for the same reason. The panel is opened with
/// `M`, so binding lowercase `m` to [`RenderOptions::MINIMAL`] - every field off - would let
/// pressing the opening key twice silently wipe the whole setting and persist the result. Digits
/// cannot collide with the key that opens the panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RenderOptionsDialog {
    options: RenderOptions,
    /// What `options` was when the panel opened, for `Esc` to restore.
    initial: RenderOptions,
    selected: usize,
}

impl RenderOptionsDialog {
    pub fn new(options: RenderOptions) -> Self {
        Self {
            options,
            initial: options,
            selected: 0,
        }
    }

    /// The options this panel was opened with. `app.rs` restores these when the panel is
    /// cancelled, since every change made inside it has already been applied and persisted.
    pub fn initial(&self) -> RenderOptions {
        self.initial
    }

    fn row_count(&self) -> usize {
        self.options.options().len()
    }

    /// Centered popup, sized to fit every option row plus the hint line - same centering formula
    /// as `ThemeDialog::popup_area`.
    ///
    /// The width is the wider of the option rows and [`HINT`], rather than a constant: at 56 the
    /// hint was cut off after `2: ful` and `Esc` - the one key a reader opens the panel not
    /// knowing - was the half that vanished. Derived, so editing the hint cannot silently truncate
    /// it again.
    pub fn popup_area(&self, area: Rect) -> Rect {
        let width = (HINT.chars().count() as u16 + 2).max(56).min(area.width);
        let height = (self.row_count() as u16 + 3).min(area.height);
        let x = area.x + (area.width.saturating_sub(width)) / 2;
        let y = area.y + (area.height.saturating_sub(height)) / 2;
        Rect::new(x, y, width, height)
    }
}

impl Component for RenderOptionsDialog {
    fn handle_key_event(&mut self, key: KeyEvent) -> Result<Option<Action>> {
        match key.code {
            KeyCode::Up => {
                let len = self.row_count();
                move_selection(&mut self.selected, -1, len);
                Ok(Some(Action::Render))
            }
            KeyCode::Down => {
                let len = self.row_count();
                move_selection(&mut self.selected, 1, len);
                Ok(Some(Action::Render))
            }
            KeyCode::Char(' ') => {
                self.options.toggle(self.selected);
                Ok(Some(Action::RenderOptionsChanged(self.options)))
            }
            KeyCode::Enter => Ok(Some(Action::RenderOptionsAccepted)),
            KeyCode::Char('1') => {
                self.options = RenderOptions::MINIMAL;
                Ok(Some(Action::RenderOptionsChanged(self.options)))
            }
            KeyCode::Char('2') => {
                self.options = RenderOptions::FULL;
                Ok(Some(Action::RenderOptionsChanged(self.options)))
            }
            KeyCode::Esc => Ok(Some(Action::DialogCancelled)),
            _ => Ok(None),
        }
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> Result<()> {
        let items = self
            .options
            .options()
            .into_iter()
            .map(|(label, on)| ListItem::new(format!("[{}] {label}", if on { 'x' } else { ' ' })))
            .collect();
        render_list_dialog(
            frame,
            area,
            Line::from("Render options"),
            items,
            self.selected,
            HINT,
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEventKind, KeyEventState, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    #[test]
    fn space_toggles_the_selected_option_and_reports_it() {
        let mut dialog = RenderOptionsDialog::new(RenderOptions::MINIMAL);

        let action = dialog.handle_key_event(key(KeyCode::Char(' '))).unwrap();

        assert_eq!(
            action,
            Some(Action::RenderOptionsChanged(RenderOptions {
                leading_whitespace: true,
                structural_punctuation: false,
                whole_pair_updates: false,
                paint_reindent_only_moves: false,
                paint_displaced_moves: false,
                paint_resized_moves: false,
            }))
        );
    }

    #[test]
    fn down_then_space_toggles_the_second_option() {
        let mut dialog = RenderOptionsDialog::new(RenderOptions::MINIMAL);
        dialog.handle_key_event(key(KeyCode::Down)).unwrap();

        let action = dialog.handle_key_event(key(KeyCode::Char(' '))).unwrap();

        assert_eq!(
            action,
            Some(Action::RenderOptionsChanged(RenderOptions {
                leading_whitespace: false,
                structural_punctuation: true,
                whole_pair_updates: false,
                paint_reindent_only_moves: false,
                paint_displaced_moves: false,
                paint_resized_moves: false,
            }))
        );
    }

    #[test]
    fn digits_jump_straight_to_the_named_presets() {
        let mut dialog = RenderOptionsDialog::new(RenderOptions::FULL);

        let to_minimal = dialog.handle_key_event(key(KeyCode::Char('1'))).unwrap();
        assert_eq!(
            to_minimal,
            Some(Action::RenderOptionsChanged(RenderOptions::MINIMAL))
        );

        let to_full = dialog.handle_key_event(key(KeyCode::Char('2'))).unwrap();
        assert_eq!(
            to_full,
            Some(Action::RenderOptionsChanged(RenderOptions::FULL))
        );
    }

    /// `Enter` accepts: it closes the panel and reports nothing to change, because every toggle
    /// already applied itself on the way in. Until 2026-09-18 it was a second toggle key and no
    /// key accepted at all, so `Esc` (which reverts) and `q` (which quits the application) were
    /// the only ways out of the panel.
    #[test]
    fn enter_accepts_and_changes_nothing_on_the_way_out() {
        let mut dialog = RenderOptionsDialog::new(RenderOptions::MINIMAL);
        dialog.handle_key_event(key(KeyCode::Char(' '))).unwrap();
        let toggled = dialog.options;

        let action = dialog.handle_key_event(key(KeyCode::Enter)).unwrap();

        assert_eq!(action, Some(Action::RenderOptionsAccepted));
        assert_eq!(
            dialog.options, toggled,
            "accepting must not edit the options it accepts"
        );
    }

    /// The panel opens on `M`, so a stray lowercase `m` inside it must do nothing: bound to
    /// MINIMAL it would turn every field off, applied and persisted, before the user could
    /// react.
    #[test]
    fn the_key_that_opens_the_panel_is_inert_inside_it() {
        let mut dialog = RenderOptionsDialog::new(RenderOptions::FULL);

        assert_eq!(
            dialog.handle_key_event(key(KeyCode::Char('m'))).unwrap(),
            None
        );
        assert_eq!(
            dialog.handle_key_event(key(KeyCode::Char('f'))).unwrap(),
            None
        );
        assert_eq!(dialog.options, RenderOptions::FULL);
    }

    #[test]
    fn esc_cancels_without_changing_anything() {
        let mut dialog = RenderOptionsDialog::new(RenderOptions::FULL);

        let action = dialog.handle_key_event(key(KeyCode::Esc)).unwrap();

        assert_eq!(action, Some(Action::DialogCancelled));
        assert_eq!(dialog.options, RenderOptions::FULL);
    }

    /// `Esc` after a change is the case that matters: the change is already on disk, so the panel
    /// has to remember what it opened with for `app.rs` to put back. The test above passes
    /// vacuously - nothing was changed before pressing Esc - so it cannot catch a lost `initial`.
    #[test]
    fn esc_after_a_change_still_reports_what_the_panel_opened_with() {
        let mut dialog = RenderOptionsDialog::new(RenderOptions::FULL);

        dialog.handle_key_event(key(KeyCode::Char('1'))).unwrap();
        assert_eq!(dialog.options, RenderOptions::MINIMAL);

        let action = dialog.handle_key_event(key(KeyCode::Esc)).unwrap();

        assert_eq!(action, Some(Action::DialogCancelled));
        assert_eq!(dialog.initial(), RenderOptions::FULL);
    }

    #[test]
    fn selection_does_not_move_past_the_last_row() {
        let mut dialog = RenderOptionsDialog::new(RenderOptions::FULL);
        for _ in 0..10 {
            dialog.handle_key_event(key(KeyCode::Down)).unwrap();
        }
        assert_eq!(dialog.selected, dialog.row_count() - 1);
    }
}
