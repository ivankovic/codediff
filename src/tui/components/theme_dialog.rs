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
 *  You should have received a copy of the GNU Affero General License
 *  along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::Rect,
    prelude::Stylize,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};
use strum::IntoEnumIterator;

use super::{Component, move_selection};
use crate::tui::actions::Action;
use crate::tui::theme::{
    CustomPalette, OverlayTheme, parse_hex_color, save_custom_palette, save_syntax_theme,
    set_custom_palette,
};
use crate::tui::widgets::code_viewer::syntax_theme_names;

/// One editable color in the dialog, and the [`CustomPalette`] field it reads and writes.
///
/// Ordered as the dialog displays them: the four diff operations first (the colors a user is most
/// likely to want to change), then the two cursor/search accents, then the panel titles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ColorSlot {
    Insert,
    Delete,
    Update,
    Move,
    CursorCounterpart,
    Search,
    OverlayText,
    BeforeTitle,
    AfterTitle,
}

impl ColorSlot {
    const ALL: [ColorSlot; 9] = [
        ColorSlot::Insert,
        ColorSlot::Delete,
        ColorSlot::Update,
        ColorSlot::Move,
        ColorSlot::CursorCounterpart,
        ColorSlot::Search,
        ColorSlot::OverlayText,
        ColorSlot::BeforeTitle,
        ColorSlot::AfterTitle,
    ];

    fn label(self) -> &'static str {
        match self {
            ColorSlot::Insert => "Insert",
            ColorSlot::Delete => "Delete",
            ColorSlot::Update => "Update",
            ColorSlot::Move => "Move",
            ColorSlot::CursorCounterpart => "Cursor counterpart",
            ColorSlot::Search => "Search match",
            ColorSlot::OverlayText => "Overlay text",
            ColorSlot::BeforeTitle => "\"Before\" title",
            ColorSlot::AfterTitle => "\"After\" title",
        }
    }

    fn get(self, palette: &CustomPalette) -> &str {
        match self {
            ColorSlot::Insert => &palette.insert_bg,
            ColorSlot::Delete => &palette.delete_bg,
            ColorSlot::Update => &palette.update_bg,
            ColorSlot::Move => &palette.move_bg,
            ColorSlot::CursorCounterpart => &palette.cross_highlight_bg,
            ColorSlot::Search => &palette.search_bg,
            ColorSlot::OverlayText => &palette.overlay_fg,
            ColorSlot::BeforeTitle => &palette.before_title_fg,
            ColorSlot::AfterTitle => &palette.after_title_fg,
        }
    }

    fn set(self, palette: &mut CustomPalette, hex: String) {
        let field = match self {
            ColorSlot::Insert => &mut palette.insert_bg,
            ColorSlot::Delete => &mut palette.delete_bg,
            ColorSlot::Update => &mut palette.update_bg,
            ColorSlot::Move => &mut palette.move_bg,
            ColorSlot::CursorCounterpart => &mut palette.cross_highlight_bg,
            ColorSlot::Search => &mut palette.search_bg,
            ColorSlot::OverlayText => &mut palette.overlay_fg,
            ColorSlot::BeforeTitle => &mut palette.before_title_fg,
            ColorSlot::AfterTitle => &mut palette.after_title_fg,
        };
        *field = hex;
    }
}

/// The dialog's rows, in display order: two dropdowns, then one row per [`ColorSlot`].
const DROPDOWN_ROWS: usize = 2;
const THEME_ROW: usize = 0;
const SYNTAX_ROW: usize = 1;

/// The theme editor (the `c` key).
///
/// Replaced a plain eight-item theme list on 2026-08-24. The list could only answer "which preset",
/// and every color in it was fixed; this shows the selected theme's actual colors and lets each one
/// be edited in place, so the preset list becomes a set of starting points rather than the whole
/// choice.
///
/// **Editing any color forks to `OverlayTheme::Custom`.** Presets stay exactly as published - a
/// user who edits Dracula's insert color gets a Custom palette seeded from Dracula, and Dracula
/// itself is unchanged the next time they select it. There is one custom palette, not one per
/// preset, so "fork, edit, fork again from a different preset" overwrites rather than accumulating.
pub struct ThemeDialog {
    themes: Vec<OverlayTheme>,
    theme_index: usize,
    syntax_themes: Vec<String>,
    syntax_index: usize,
    /// The colors shown in the color rows. Mirrors whichever theme is selected, and is what gets
    /// saved as the custom palette once edited.
    working: CustomPalette,
    /// Which row has focus: `THEME_ROW`, `SYNTAX_ROW`, or `DROPDOWN_ROWS + n` for the nth color.
    selected_row: usize,
    /// `Some` while a hex value is being typed into the focused color row.
    editing: Option<String>,
}

impl ThemeDialog {
    /// Create the dialog with `current` selected and its colors loaded into the editable rows.
    pub fn new(current: OverlayTheme) -> Self {
        Self::with_syntax_theme(current, None)
    }

    /// Same, but pre-selecting a syntax-highlighting theme by name.
    pub fn with_syntax_theme(current: OverlayTheme, syntax_theme: Option<&str>) -> Self {
        let themes: Vec<OverlayTheme> = OverlayTheme::iter().collect();
        let theme_index = themes.iter().position(|&t| t == current).unwrap_or(0);
        let syntax_themes = syntax_theme_names();
        let syntax_index = syntax_theme
            .and_then(|name| syntax_themes.iter().position(|t| t == name))
            .unwrap_or(0);
        Self {
            themes,
            theme_index,
            syntax_themes,
            syntax_index,
            working: CustomPalette::from_palette(&current.palette()),
            selected_row: THEME_ROW,
            editing: None,
        }
    }

    fn row_count(&self) -> usize {
        DROPDOWN_ROWS + ColorSlot::ALL.len()
    }

    /// The color slot the focused row edits, or `None` on a dropdown row.
    fn focused_slot(&self) -> Option<ColorSlot> {
        self.selected_row
            .checked_sub(DROPDOWN_ROWS)
            .and_then(|i| ColorSlot::ALL.get(i).copied())
    }

    fn theme(&self) -> OverlayTheme {
        self.themes[self.theme_index]
    }

    /// Move the theme dropdown and reload the color rows from the newly selected theme, so the
    /// rows always describe what the viewer behind the dialog is showing.
    fn cycle_theme(&mut self, delta: i32) -> Action {
        move_selection(&mut self.theme_index, delta, self.themes.len());
        let theme = self.theme();
        self.working = CustomPalette::from_palette(&theme.palette());
        Action::ThemePreviewed(theme)
    }

    fn cycle_syntax(&mut self, delta: i32) -> Option<Action> {
        if self.syntax_themes.is_empty() {
            return None;
        }
        move_selection(&mut self.syntax_index, delta, self.syntax_themes.len());
        Some(Action::SyntaxThemePreviewed(
            self.syntax_themes[self.syntax_index].clone(),
        ))
    }

    /// Commit a typed hex value: fork to Custom, install it for live preview, and re-preview.
    ///
    /// An unparseable value is rejected rather than stored, so a half-typed `#ff` cannot become a
    /// persisted color. The row simply keeps its previous value.
    fn commit_edit(&mut self, slot: ColorSlot, typed: String) -> Option<Action> {
        parse_hex_color(&typed)?;
        slot.set(&mut self.working, normalize_hex(&typed));
        set_custom_palette(self.working.clone());
        self.theme_index = self
            .themes
            .iter()
            .position(|&t| t == OverlayTheme::Custom)
            .unwrap_or(self.theme_index);
        Some(Action::ThemePreviewed(OverlayTheme::Custom))
    }

    /// Persist everything the dialog owns and report the chosen overlay theme.
    ///
    /// The custom palette and the syntax theme are saved here rather than in `app.rs`'s
    /// `ThemeSelected` handler: both are dialog-local state that no `Action` currently carries,
    /// and inventing two more actions to move them one level up would not make them any less
    /// dialog-owned.
    fn commit_dialog(&self) -> Action {
        save_custom_palette(self.working.clone());
        if let Some(name) = self.syntax_themes.get(self.syntax_index) {
            save_syntax_theme(name);
        }
        Action::ThemeSelected(self.theme())
    }

    /// The area the popup occupies, centered within `area` and sized to fit every row plus the
    /// border and hint line.
    pub fn popup_area(&self, area: Rect) -> Rect {
        let width = 52.min(area.width);
        let height = (self.row_count() as u16 + 2).min(area.height);
        let x = area.x + (area.width.saturating_sub(width)) / 2;
        let y = area.y + (area.height.saturating_sub(height)) / 2;
        Rect::new(x, y, width, height)
    }
}

/// `#rrggbb`, lowercase, with the `#` present however it was typed.
fn normalize_hex(typed: &str) -> String {
    format!("#{}", typed.trim().trim_start_matches('#').to_lowercase())
}

impl Component for ThemeDialog {
    fn handle_key_event(&mut self, key: KeyEvent) -> Result<Option<Action>> {
        // Hex entry swallows every key while active: a bare `d` is a hex digit here, not a
        // navigation key, and Esc must cancel the edit rather than the whole dialog.
        if let Some(buffer) = self.editing.as_mut() {
            match key.code {
                KeyCode::Char(c) if c.is_ascii_hexdigit() || c == '#' => {
                    if buffer.trim_start_matches('#').len() < 6 || c == '#' {
                        buffer.push(c);
                    }
                    return Ok(Some(Action::Render));
                }
                KeyCode::Backspace => {
                    buffer.pop();
                    return Ok(Some(Action::Render));
                }
                KeyCode::Enter => {
                    let typed = buffer.clone();
                    self.editing = None;
                    let slot = self.focused_slot();
                    return Ok(slot
                        .and_then(|slot| self.commit_edit(slot, typed))
                        .or(Some(Action::Render)));
                }
                KeyCode::Esc => {
                    self.editing = None;
                    return Ok(Some(Action::Render));
                }
                _ => return Ok(Some(Action::Render)),
            }
        }

        match key.code {
            KeyCode::Up => {
                let rows = self.row_count();
                move_selection(&mut self.selected_row, -1, rows);
                Ok(Some(Action::Render))
            }
            KeyCode::Down => {
                let rows = self.row_count();
                move_selection(&mut self.selected_row, 1, rows);
                Ok(Some(Action::Render))
            }
            KeyCode::Left | KeyCode::Right => {
                let delta = if key.code == KeyCode::Left { -1 } else { 1 };
                match self.selected_row {
                    THEME_ROW => Ok(Some(self.cycle_theme(delta))),
                    SYNTAX_ROW => Ok(self.cycle_syntax(delta)),
                    _ => Ok(None),
                }
            }
            // Enter opens the hex field on a color row, and commits the dialog anywhere else -
            // so a user who only wants to switch preset never has to learn the editing keys.
            KeyCode::Enter => match self.focused_slot() {
                Some(slot) => {
                    self.editing = Some(slot.get(&self.working).to_string());
                    Ok(Some(Action::Render))
                }
                None => Ok(Some(self.commit_dialog())),
            },
            KeyCode::Esc => Ok(Some(Action::DialogCancelled)),
            _ => Ok(None),
        }
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> Result<()> {
        let mut lines: Vec<Line> = Vec::with_capacity(self.row_count() + 1);
        let marker = |row: usize, selected: usize| if row == selected { "> " } else { "  " };

        lines.push(Line::from(vec![
            Span::raw(marker(THEME_ROW, self.selected_row)),
            Span::raw(format!("{:<20}", "Theme:")),
            Span::styled(
                format!("< {} >", self.theme()),
                Style::new().fg(Color::Cyan),
            ),
        ]));
        let syntax_name = self
            .syntax_themes
            .get(self.syntax_index)
            .map(String::as_str)
            .unwrap_or("(none)");
        lines.push(Line::from(vec![
            Span::raw(marker(SYNTAX_ROW, self.selected_row)),
            Span::raw(format!("{:<20}", "Syntax theme:")),
            Span::styled(format!("< {syntax_name} >"), Style::new().fg(Color::Cyan)),
        ]));

        for (index, slot) in ColorSlot::ALL.iter().enumerate() {
            let row = DROPDOWN_ROWS + index;
            let stored = slot.get(&self.working).to_string();
            // While editing, the swatch tracks what has been typed so far when it parses, so the
            // color updates under the cursor instead of only on Enter.
            let shown = match (&self.editing, row == self.selected_row) {
                (Some(buffer), true) => buffer.clone(),
                _ => stored.clone(),
            };
            let swatch = parse_hex_color(&shown)
                .or_else(|| parse_hex_color(&stored))
                .unwrap_or(Color::Reset);
            let value = if row == self.selected_row && self.editing.is_some() {
                format!("{shown}_")
            } else {
                shown
            };
            lines.push(Line::from(vec![
                Span::raw(marker(row, self.selected_row)),
                Span::raw(format!("{:<20}", slot.label())),
                Span::styled("███", Style::new().fg(swatch)),
                Span::raw("  "),
                Span::raw(value),
            ]));
        }

        let hint = if self.editing.is_some() {
            " type #rrggbb | Enter: apply | Esc: cancel edit "
        } else {
            " ←/→: change | Enter: edit or accept | Esc: cancel "
        };

        frame.render_widget(Clear, area);
        frame.render_widget(
            Paragraph::new(lines).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(Line::from(" Theme ".bold().fg(Color::Cyan)))
                    .title_bottom(Line::from(hint)),
            ),
            area,
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn typing(dialog: &mut ThemeDialog, text: &str) {
        for c in text.chars() {
            dialog.handle_key_event(key(KeyCode::Char(c))).unwrap();
        }
    }

    /// Move focus down to the first color row (Insert).
    fn focus_first_color(dialog: &mut ThemeDialog) {
        for _ in 0..DROPDOWN_ROWS {
            dialog.handle_key_event(key(KeyCode::Down)).unwrap();
        }
    }

    #[test]
    fn new_preselects_the_current_theme_and_loads_its_colors() {
        let dialog = ThemeDialog::new(OverlayTheme::SolarizedDark);
        assert_eq!(dialog.theme(), OverlayTheme::SolarizedDark);
        assert_eq!(
            dialog.working,
            CustomPalette::from_palette(&OverlayTheme::SolarizedDark.palette())
        );
    }

    /// The theme row is a dropdown now: left/right cycles it, and the color rows must follow, or
    /// they would keep describing the previously selected theme.
    #[test]
    fn cycling_the_theme_reloads_the_color_rows() {
        let mut dialog = ThemeDialog::new(OverlayTheme::Dark);
        let action = dialog.handle_key_event(key(KeyCode::Right)).unwrap();
        let theme = dialog.theme();
        assert_ne!(theme, OverlayTheme::Dark);
        assert_eq!(action, Some(Action::ThemePreviewed(theme)));
        assert_eq!(
            dialog.working,
            CustomPalette::from_palette(&theme.palette())
        );
    }

    /// Editing any color forks to Custom rather than mutating the preset.
    #[test]
    fn editing_a_color_switches_the_selection_to_custom() {
        let mut dialog = ThemeDialog::new(OverlayTheme::Dracula);
        focus_first_color(&mut dialog);
        dialog.handle_key_event(key(KeyCode::Enter)).unwrap();
        // Replace the pre-filled value entirely.
        for _ in 0..8 {
            dialog.handle_key_event(key(KeyCode::Backspace)).unwrap();
        }
        typing(&mut dialog, "#123456");
        let action = dialog.handle_key_event(key(KeyCode::Enter)).unwrap();

        assert_eq!(action, Some(Action::ThemePreviewed(OverlayTheme::Custom)));
        assert_eq!(dialog.theme(), OverlayTheme::Custom);
        assert_eq!(dialog.working.insert_bg, "#123456");
        // The preset itself is untouched.
        assert_ne!(
            OverlayTheme::Dracula.palette().insert_bg,
            parse_hex_color("#123456").unwrap()
        );
    }

    /// A half-typed value must not become a stored color.
    #[test]
    fn an_unparseable_hex_value_is_rejected() {
        let mut dialog = ThemeDialog::new(OverlayTheme::Dracula);
        let before = dialog.working.insert_bg.clone();
        focus_first_color(&mut dialog);
        dialog.handle_key_event(key(KeyCode::Enter)).unwrap();
        for _ in 0..8 {
            dialog.handle_key_event(key(KeyCode::Backspace)).unwrap();
        }
        typing(&mut dialog, "#ff");
        dialog.handle_key_event(key(KeyCode::Enter)).unwrap();

        assert_eq!(dialog.working.insert_bg, before);
        assert_eq!(dialog.theme(), OverlayTheme::Dracula);
    }

    /// Esc while typing cancels the edit only - the dialog stays open. This is the one place two
    /// Escs mean different things, so it is worth pinning.
    #[test]
    fn esc_while_editing_cancels_the_edit_not_the_dialog() {
        let mut dialog = ThemeDialog::new(OverlayTheme::Dracula);
        focus_first_color(&mut dialog);
        dialog.handle_key_event(key(KeyCode::Enter)).unwrap();
        typing(&mut dialog, "aabbcc");
        let action = dialog.handle_key_event(key(KeyCode::Esc)).unwrap();
        assert_eq!(action, Some(Action::Render));
        assert!(dialog.editing.is_none());

        let action = dialog.handle_key_event(key(KeyCode::Esc)).unwrap();
        assert_eq!(action, Some(Action::DialogCancelled));
    }

    /// Enter on a dropdown row accepts the dialog, so switching preset stays a two-key operation.
    #[test]
    fn enter_on_a_dropdown_row_accepts_the_dialog() {
        let mut dialog = ThemeDialog::new(OverlayTheme::Dracula);
        let action = dialog.handle_key_event(key(KeyCode::Enter)).unwrap();
        assert_eq!(action, Some(Action::ThemeSelected(OverlayTheme::Dracula)));
    }

    #[test]
    fn navigation_stops_at_both_ends() {
        let mut dialog = ThemeDialog::new(OverlayTheme::Dracula);
        dialog.handle_key_event(key(KeyCode::Up)).unwrap();
        assert_eq!(dialog.selected_row, 0);
        for _ in 0..50 {
            dialog.handle_key_event(key(KeyCode::Down)).unwrap();
        }
        assert_eq!(dialog.selected_row, dialog.row_count() - 1);
    }

    #[test]
    fn every_color_slot_round_trips_through_the_palette() {
        let mut palette = CustomPalette::default();
        for slot in ColorSlot::ALL {
            slot.set(&mut palette, "#0a0b0c".to_string());
            assert_eq!(slot.get(&palette), "#0a0b0c", "{:?}", slot);
        }
    }

    /// Render the dialog into a test buffer and dump it, so the layout is verified as text rather
    /// than assumed from the row model. Catches a dropdown or swatch that silently renders empty.
    #[test]
    fn renders_every_row_with_a_value() {
        use ratatui::{Terminal, backend::TestBackend};

        let mut dialog = ThemeDialog::new(OverlayTheme::Dracula);
        let mut terminal = Terminal::new(TestBackend::new(60, 20)).expect("terminal");
        terminal
            .draw(|frame| {
                let area = dialog.popup_area(frame.size());
                dialog.draw(frame, area).expect("draw");
            })
            .expect("render");

        let rendered: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .enumerate()
            .fold(String::new(), |mut acc, (i, cell)| {
                if i % 60 == 0 && i > 0 {
                    acc.push('\n');
                }
                acc.push_str(cell.symbol());
                acc
            });

        assert!(rendered.contains("Theme:"), "{rendered}");
        assert!(rendered.contains("Dracula"), "{rendered}");
        assert!(rendered.contains("Syntax theme:"), "{rendered}");
        for slot in ColorSlot::ALL {
            assert!(
                rendered.contains(slot.label()),
                "row {:?} missing from:\n{rendered}",
                slot
            );
        }
        // Every color row shows a concrete hex value, not an empty or `#` placeholder.
        assert_eq!(
            rendered.matches('#').count(),
            ColorSlot::ALL.len(),
            "expected one hex value per color row:\n{rendered}"
        );
    }

    #[test]
    fn hex_round_trips() {
        assert_eq!(
            crate::tui::theme::format_hex_color(parse_hex_color("#1e2f3a").unwrap()),
            "#1e2f3a"
        );
        assert!(parse_hex_color("#12345").is_none());
        assert!(parse_hex_color("nothex").is_none());
    }
}
