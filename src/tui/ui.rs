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
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    prelude::Alignment,
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::tui::app::{App, DiffStatus, Panel, Theme};
use crate::tui::widgets::code_panel::CodePanel;

/// Main UI rendering function
pub fn ui(f: &mut Frame, app: &App) {
    let size = f.size();

    // Check if terminal is in narrow mode (< 220 characters wide)
    let is_narrow = size.width < 220;

    if is_narrow {
        render_narrow_mode(f, app);
    } else {
        render_wide_mode(f, app);
    }
}

fn render_narrow_mode(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Header
            Constraint::Min(1),    // Code panel
            Constraint::Length(1), // Footer
        ])
        .split(f.size());

    // Header
    let header_spans = match app.active_panel {
        Panel::Before => vec![
            Span::styled("[", Style::default().fg(ratatui::style::Color::Red)),
            Span::styled(
                "Before",
                Style::default()
                    .fg(ratatui::style::Color::Red)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("]", Style::default().fg(ratatui::style::Color::Red)),
            Span::styled(" | ", Style::default().fg(app.colors.header_fg)),
            Span::styled("After", Style::default().fg(ratatui::style::Color::Green)),
        ],
        Panel::After => vec![
            Span::styled("Before", Style::default().fg(ratatui::style::Color::Red)),
            Span::styled(" | ", Style::default().fg(app.colors.header_fg)),
            Span::styled("[", Style::default().fg(ratatui::style::Color::Green)),
            Span::styled(
                "After",
                Style::default()
                    .fg(ratatui::style::Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("]", Style::default().fg(ratatui::style::Color::Green)),
        ],
    };
    let header = Paragraph::new(ratatui::text::Line::from(header_spans))
        .block(Block::default().borders(Borders::NONE));
    f.render_widget(header, chunks[0]);

    // Create code panel using the CodePanel widget
    let code_text = match app.active_panel {
        Panel::Before => &app.before_code,
        Panel::After => &app.after_code,
    };

    let panel_title = match app.active_panel {
        Panel::Before => "Before",
        Panel::After => "After",
    };

    let code_panel = CodePanel::new(
        panel_title.to_string(),
        code_text.clone(),
        0, // line_number starts from 0
        app.cursor_line,
        app.cursor_char,
        app.scroll_offset,
        true, // is_active - narrow mode only shows one panel
        app.colors,
        app.token_diff_ranges.clone(),
    );

    let visible_lines = chunks[1].height as usize;
    let code_paragraph = code_panel.to_widget(visible_lines);
    f.render_widget(code_paragraph, chunks[1]);

    // Footer - show diff status instead of keyboard shortcuts
    let footer_text = match &app.diff_status {
        DiffStatus::Success => {
            if let Some(cost) = app.diff_cost {
                format!("Diff: cost {}", cost)
            } else {
                "Diff: computed".to_string()
            }
        }
        DiffStatus::Error(err) => format!("Diff: error - {}", err),
        DiffStatus::NoAstDiff => "Diff: no AST diff available".to_string(),
    };
    let footer = Paragraph::new(footer_text)
        .style(Style::default().fg(app.colors.footer_fg))
        .block(Block::default().borders(Borders::NONE));
    f.render_widget(footer, chunks[2]);

    // AST Popup
    if app.show_ast_popup {
        render_ast_popup(f, app);
    }

    // Help Panel
    if app.show_help {
        render_help_popup(f, app);
    }

    // Legend
    if app.show_legend {
        render_legend(f, app);
    }

    // File Selector
    if app.show_file_selector {
        render_file_selector(f, app);
    }
}



fn render_wide_mode(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(50), // Before panel
            Constraint::Percentage(50), // After panel
        ])
        .split(f.size());

    // Before panel - use CodePanel widget
    let before_panel = CodePanel::new(
        "Before".to_string(),
        app.before_code.clone(),
        0, // line_number starts from 0
        app.cursor_line,
        app.cursor_char,
        app.scroll_offset,
        app.active_panel == Panel::Before,
        app.colors,
        app.token_diff_ranges.clone(),
    );

    let before_visible_lines = chunks[0].height as usize;
    let before_paragraph = before_panel.to_widget(before_visible_lines);
    f.render_widget(before_paragraph, chunks[0]);

    // After panel - use CodePanel widget
    let after_panel = CodePanel::new(
        "After".to_string(),
        app.after_code.clone(),
        0, // line_number starts from 0
        app.cursor_line,
        app.cursor_char,
        app.scroll_offset,
        app.active_panel == Panel::After,
        app.colors,
        app.token_diff_ranges.clone(),
    );

    let after_visible_lines = chunks[1].height as usize;
    let after_paragraph = after_panel.to_widget(after_visible_lines);
    f.render_widget(after_paragraph, chunks[1]);

    // Footer - show diff status instead of keyboard shortcuts
    let footer_area = Rect {
        x: 0,
        y: f.size().height - 1,
        width: f.size().width,
        height: 1,
    };
    let footer_text = match &app.diff_status {
        DiffStatus::Success => {
            if let Some(cost) = app.diff_cost {
                format!("Diff: cost {}", cost)
            } else {
                "Diff: computed".to_string()
            }
        }
        DiffStatus::Error(err) => format!("Diff: error - {}", err),
        DiffStatus::NoAstDiff => "Diff: no AST diff available".to_string(),
    };
    let footer = Paragraph::new(footer_text)
        .style(Style::default().fg(app.colors.footer_fg))
        .block(Block::default().borders(Borders::NONE));
    f.render_widget(footer, footer_area);

    // AST Popup
    if app.show_ast_popup {
        render_ast_popup(f, app);
    }
}

fn render_ast_popup(f: &mut Frame, app: &App) {
    // Calculate popup size with margins
    let margin = 1;
    let popup_width = f.size().width.saturating_sub(2 * margin);
    let popup_height = f.size().height.saturating_sub(2 * margin + 1); // Account for title bar + footer

    let popup_area = Rect {
        x: margin,
        y: margin + 1, // Start below the code header
        width: popup_width,
        height: popup_height,
    };

    // Build AST tree content
    let ast_content = if app.ast_path.is_empty() {
        "Move cursor to see AST path\nCurrent position: Line {}, Char {}".to_string()
    } else {
        // Show proper tree structure
        let mut tree_text = String::new();
        for (i, node) in app.ast_path.iter().enumerate() {
            // Add indentation based on depth
            let indent = "  ".repeat(i);
            if i == app.ast_path.len() - 1 {
                tree_text.push_str(&format!("{}└─ {}\n", indent, node));
            } else {
                tree_text.push_str(&format!("{}├─ {}\n", indent, node));
            }
        }
        tree_text
    };

    let popup = Paragraph::new(ast_content)
        .block(
            Block::default()
                .title("AST Tree")
                .borders(Borders::ALL)
                .style(Style::default().fg(app.colors.popup_border)),
        )
        .style(
            Style::default()
                .fg(app.colors.popup_fg)
                .bg(app.colors.popup_bg),
        )
        .alignment(Alignment::Left);

    f.render_widget(Clear, popup_area); // Clear the area first
    f.render_widget(popup, popup_area);
}

fn render_help_popup(f: &mut Frame, app: &App) {
    // Calculate popup size and position
    let margin = 2;
    let popup_width = f.size().width.saturating_sub(2 * margin);
    let popup_height = f.size().height.saturating_sub(2 * margin + 1);

    let popup_area = Rect {
        x: margin,
        y: margin,
        width: popup_width,
        height: popup_height,
    };

    let help_content = "
CodeDiff Keyboard Shortcuts:

Navigation:
  ↑, ↓, ←, →  Navigate cursor (or hjkl)
  Tab        Switch between Before/After panels
  Space      Align both panels (TODO)

View:
  t          Toggle AST tree popup
  ?          Toggle this help panel
  l          Toggle color legend
  c          Toggle light/dark theme
  ESC        Close all popups

AST:
  t          Show AST tree at cursor position

Quit:
  q          Quit application

Current Theme: "
        .to_string()
        + match app.theme {
            Theme::Light => "Light",
            Theme::Dark => "Dark",
        };

    let popup = Paragraph::new(help_content)
        .block(
            Block::default()
                .title("Help - Keyboard Shortcuts")
                .borders(Borders::ALL)
                .style(Style::default().fg(app.colors.popup_border)),
        )
        .style(
            Style::default()
                .fg(app.colors.popup_fg)
                .bg(app.colors.popup_bg),
        )
        .alignment(ratatui::prelude::Alignment::Left);

    f.render_widget(Clear, popup_area);
    f.render_widget(popup, popup_area);
}

fn render_legend(f: &mut Frame, app: &App) {
    // Position legend in lower right corner
    let legend_width = 35;
    let legend_height = 12;
    let legend_x = f.size().width.saturating_sub(legend_width + 2);
    let legend_y = f.size().height.saturating_sub(legend_height + 1);

    let legend_area = Rect {
        x: legend_x,
        y: legend_y,
        width: legend_width,
        height: legend_height,
    };

    // Create colored spans for each colour type
    let added_text = Span::styled("Added", Style::default().fg(app.colors.diff_added));
    let removed_text = Span::styled("Removed", Style::default().fg(app.colors.diff_removed));
    let changed_text = Span::styled("Changed", Style::default().fg(app.colors.diff_changed));
    let unchanged_text = Span::styled("Unchanged", Style::default().fg(app.colors.text));

    let legend_content = vec![
        ratatui::text::Line::from(vec![
            added_text,
            Span::styled("    : New code", Style::default().fg(app.colors.popup_fg)),
        ]),
        ratatui::text::Line::from(vec![
            removed_text,
            Span::styled("  : Deleted code", Style::default().fg(app.colors.popup_fg)),
        ]),
        ratatui::text::Line::from(vec![
            changed_text,
            Span::styled(" : Modified code", Style::default().fg(app.colors.popup_fg)),
        ]),
        ratatui::text::Line::from(vec![
            unchanged_text,
            Span::styled("  : Common code", Style::default().fg(app.colors.popup_fg)),
        ]),
        ratatui::text::Line::from(""),
        ratatui::text::Line::from(format!(
            "Theme: {}",
            match app.theme {
                Theme::Light => "Light",
                Theme::Dark => "Dark",
            }
        )),
    ];

    let legend = Paragraph::new(legend_content)
        .block(
            Block::default()
                .title("Colour Legend")
                .borders(Borders::ALL)
                .style(Style::default().fg(app.colors.popup_border)),
        )
        .style(
            Style::default()
                .fg(app.colors.popup_fg)
                .bg(app.colors.popup_bg),
        )
        .alignment(ratatui::prelude::Alignment::Left);

    f.render_widget(Clear, legend_area);
    f.render_widget(legend, legend_area);
}

fn render_file_selector(f: &mut Frame, app: &App) {
    // Calculate popup size and position
    let margin = 2;
    let popup_width = f.size().width.saturating_sub(2 * margin);
    let popup_height = f.size().height.saturating_sub(2 * margin + 1);

    let popup_area = Rect {
        x: margin,
        y: margin,
        width: popup_width,
        height: popup_height,
    };

    // Create title with current path
    let title = format!("File Selector - {}", app.file_selector_path);

    // Create list of entries with selection highlight
    let mut lines = Vec::new();
    for (i, entry) in app.file_selector_entries.iter().enumerate() {
        let mut line_content = entry.clone();
        
        // Add directory marker for directories
        let full_path = format!("{}/{}", app.file_selector_path, entry);
        if std::path::Path::new(&full_path).is_dir() {
            line_content = format!("📁 {}", line_content);
        }
        
        // Highlight selected entry
        if i == app.file_selector_selected {
            lines.push(Line::from(Span::styled(
                line_content,
                Style::default()
                    .fg(app.colors.popup_bg)
                    .bg(app.colors.popup_fg)
                    .add_modifier(Modifier::BOLD),
            )));
        } else {
            lines.push(Line::from(Span::styled(
                line_content,
                Style::default().fg(app.colors.popup_fg),
            )));
        }
    }

    // Add instructions at bottom
    if !app.file_selector_entries.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "↑/↓: Navigate | →: Select | ←: Up | ESC: Cancel",
            Style::default().fg(app.colors.popup_fg).add_modifier(Modifier::ITALIC),
        )));
    }

    let file_selector = Paragraph::new(Text::from(lines))
        .block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .style(Style::default().fg(app.colors.popup_border)),
        )
        .style(
            Style::default()
                .fg(app.colors.popup_fg)
                .bg(app.colors.popup_bg),
        )
        .alignment(Alignment::Left);

    f.render_widget(Clear, popup_area);
    f.render_widget(file_selector, popup_area);
}
