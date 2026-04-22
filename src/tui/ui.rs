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
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    prelude::Alignment,
};

use crate::tui::app::{App, Panel, LineDiffStatus, Theme};

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
            Span::styled("Before", Style::default().fg(ratatui::style::Color::Red).add_modifier(Modifier::BOLD)),
            Span::styled("]", Style::default().fg(ratatui::style::Color::Red)),
            Span::styled(" | ", Style::default().fg(app.colors.header_fg)),
            Span::styled("After", Style::default().fg(ratatui::style::Color::Green)),
        ],
        Panel::After => vec![
            Span::styled("Before", Style::default().fg(ratatui::style::Color::Red)),
            Span::styled(" | ", Style::default().fg(app.colors.header_fg)),
            Span::styled("[", Style::default().fg(ratatui::style::Color::Green)),
            Span::styled("After", Style::default().fg(ratatui::style::Color::Green).add_modifier(Modifier::BOLD)),
            Span::styled("]", Style::default().fg(ratatui::style::Color::Green)),
        ],
    };
    let header = Paragraph::new(ratatui::text::Line::from(header_spans))
        .block(Block::default().borders(Borders::NONE));
    f.render_widget(header, chunks[0]);

    // Code panel
    let code_text = match app.active_panel {
        Panel::Before => &app.before_code,
        Panel::After => &app.after_code,
    };

    // Calculate visible lines based on scroll offset
    let visible_lines = chunks[1].height as usize;

    let lines: Vec<Line> = code_text
        .lines()
        .enumerate()
        .skip(app.scroll_offset)
        .take(visible_lines)
        .map(|(line_idx, line)| {
            let mut spans = Vec::new();

            // Add line number prefix
            spans.push(Span::styled(
                format!("{:4} ", line_idx + 1),
                Style::default().fg(app.colors.footer_fg),
            ));

            // Highlight individual characters
            if line_idx == app.cursor_line {
                for (char_idx, c) in line.chars().enumerate() {
                    if char_idx == app.cursor_char {
                        // Highlight current character
                        spans.push(Span::styled(
                            c.to_string(),
                            Style::default()
                                .fg(app.colors.cursor_fg)
                                .bg(app.colors.cursor_bg)
                                .add_modifier(Modifier::BOLD),
                        ));
                    } else {
                        // Normal character
                        spans.push(Span::styled(
                            c.to_string(),
                            Style::default().fg(app.colors.text),
                        ));
                    }
                }
            } else {
                // Normal line
                spans.push(Span::styled(
                    line.to_string(),
                    Style::default().fg(app.colors.text),
                ));
            }

            Line::from(spans)
        })
        .collect();

    let code_paragraph = Paragraph::new(Text::from(lines))
        .block(Block::default().borders(Borders::ALL).title("Code"))
        .wrap(Wrap { trim: true });
    f.render_widget(code_paragraph, chunks[1]);

    // Footer
    let footer_text = "Arrow keys: Navigate | Space: Align | t: AST | q: Quit";
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
}

fn render_wide_mode(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(50), // Before panel
            Constraint::Percentage(50), // After panel
        ])
        .split(f.size());

    // Before panel - calculate visible lines based on scroll offset
    let before_visible_lines = chunks[0].height as usize;

    let before_lines: Vec<Line> = app
        .before_code
        .lines()
        .enumerate()
        .skip(app.scroll_offset)
        .take(before_visible_lines)
        .map(|(line_idx, line)| {
            let mut spans = Vec::new();

            // Add line number prefix
            spans.push(Span::styled(
                format!("{:4} ", line_idx + 1),
                Style::default().fg(app.colors.footer_fg),
            ));

            // Calculate byte position for this line
            let line_start_byte = app.before_code[..app
                .before_code
                .lines()
                .take(line_idx)
                .map(|l| l.len() + 1)
                .sum::<usize>()
                .saturating_sub(1)]
                .len();

            // Find token ranges that overlap with this line
            let line_ranges: Vec<_> = app
                .token_diff_ranges
                .iter()
                .filter(|&&(start, end, _)| {
                    start < line_start_byte + line.len() && end > line_start_byte
                })
                .collect();

            // Highlight individual characters with token-based coloring
            if app.active_panel == Panel::Before && line_idx == app.cursor_line {
                for (byte_idx, c) in line.char_indices() {
                    let char_byte_pos = line_start_byte + byte_idx;

                    if byte_idx == app.cursor_char {
                        // Highlight current character
                        spans.push(Span::styled(
                            c.to_string(),
                            Style::default()
                                .fg(app.colors.cursor_fg)
                                .bg(app.colors.cursor_bg)
                                .add_modifier(Modifier::BOLD),
                        ));
                    } else {
                        // Find color based on token ranges
                        let mut char_color = app.colors.text;
                        for (start, end, status) in &line_ranges {
                            if char_byte_pos >= *start && char_byte_pos < *end {
                                char_color = match status {
                                    LineDiffStatus::Added => app.colors.diff_added,
                                    LineDiffStatus::Removed => app.colors.diff_removed,
                                    LineDiffStatus::Changed => app.colors.diff_changed,
                                    LineDiffStatus::Unchanged => app.colors.text,
                                };
                                break;
                            }
                        }
                        spans.push(Span::styled(c.to_string(), Style::default().fg(char_color)));
                    }
                }
            } else {
                // Apply token-based coloring for the entire line
                for (byte_idx, c) in line.char_indices() {
                    let char_byte_pos = line_start_byte + byte_idx;

                    // Find color based on token ranges
                    let mut char_color = app.colors.text;
                    for (start, end, status) in &line_ranges {
                        if char_byte_pos >= *start && char_byte_pos < *end {
                            char_color = match status {
                                LineDiffStatus::Added => app.colors.diff_added,
                                LineDiffStatus::Removed => app.colors.diff_removed,
                                LineDiffStatus::Changed => app.colors.diff_changed,
                                LineDiffStatus::Unchanged => app.colors.text,
                            };
                            break;
                        }
                    }
                    spans.push(Span::styled(c.to_string(), Style::default().fg(char_color)));
                }
            }

            Line::from(spans)
        })
        .collect();

    let before_paragraph = Paragraph::new(Text::from(before_lines))
        .block(Block::default().borders(Borders::ALL).title("Before"))
        .wrap(Wrap { trim: true });
    f.render_widget(before_paragraph, chunks[0]);

    // After panel - calculate visible lines based on scroll offset
    let after_visible_lines = chunks[1].height as usize;

    let after_lines: Vec<Line> = app
        .after_code
        .lines()
        .enumerate()
        .skip(app.scroll_offset)
        .take(after_visible_lines)
        .map(|(line_idx, line)| {
            let mut spans = Vec::new();

            // Add line number prefix
            spans.push(Span::styled(
                format!("{:4} ", line_idx + 1),
                Style::default().fg(app.colors.footer_fg),
            ));

            // Calculate byte position for this line
            let line_start_byte = app.after_code[..app
                .after_code
                .lines()
                .take(line_idx)
                .map(|l| l.len() + 1)
                .sum::<usize>()
                .saturating_sub(1)]
                .len();

            // Find token ranges that overlap with this line
            let line_ranges: Vec<_> = app
                .token_diff_ranges
                .iter()
                .filter(|&&(start, end, _)| {
                    start < line_start_byte + line.len() && end > line_start_byte
                })
                .collect();

            // Highlight individual characters with token-based coloring
            if app.active_panel == Panel::After && line_idx == app.cursor_line {
                for (byte_idx, c) in line.char_indices() {
                    let char_byte_pos = line_start_byte + byte_idx;

                    if byte_idx == app.cursor_char {
                        // Highlight current character
                        spans.push(Span::styled(
                            c.to_string(),
                            Style::default()
                                .fg(app.colors.cursor_fg)
                                .bg(app.colors.cursor_bg)
                                .add_modifier(Modifier::BOLD),
                        ));
                    } else {
                        // Find color based on token ranges
                        let mut char_color = app.colors.text;
                        for (start, end, status) in &line_ranges {
                            if char_byte_pos >= *start && char_byte_pos < *end {
                                char_color = match status {
                                    LineDiffStatus::Added => app.colors.diff_added,
                                    LineDiffStatus::Removed => app.colors.diff_removed,
                                    LineDiffStatus::Changed => app.colors.diff_changed,
                                    LineDiffStatus::Unchanged => app.colors.text,
                                };
                                break;
                            }
                        }
                        spans.push(Span::styled(c.to_string(), Style::default().fg(char_color)));
                    }
                }
            } else {
                // Apply token-based coloring for the entire line
                for (byte_idx, c) in line.char_indices() {
                    let char_byte_pos = line_start_byte + byte_idx;

                    // Find color based on token ranges
                    let mut char_color = app.colors.text;
                    for (start, end, status) in &line_ranges {
                        if char_byte_pos >= *start && char_byte_pos < *end {
                            char_color = match status {
                                LineDiffStatus::Added => app.colors.diff_added,
                                LineDiffStatus::Removed => app.colors.diff_removed,
                                LineDiffStatus::Changed => app.colors.diff_changed,
                                LineDiffStatus::Unchanged => app.colors.text,
                            };
                            break;
                        }
                    }
                    spans.push(Span::styled(c.to_string(), Style::default().fg(char_color)));
                }
            }

            Line::from(spans)
        })
        .collect();

    let after_paragraph = Paragraph::new(Text::from(after_lines))
        .block(Block::default().borders(Borders::ALL).title("After"))
        .wrap(Wrap { trim: true });
    f.render_widget(after_paragraph, chunks[1]);

    // Footer
    let footer_area = Rect {
        x: 0,
        y: f.size().height - 1,
        width: f.size().width,
        height: 1,
    };
    let footer_text = "Arrow keys: Navigate | Tab: Switch panel | Space: Align | t: AST | q: Quit";
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

Current Theme: ".to_string() + match app.theme {
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
            Span::styled("    : New code", Style::default().fg(app.colors.popup_fg))
        ]),
        ratatui::text::Line::from(vec![
            removed_text,
            Span::styled("  : Deleted code", Style::default().fg(app.colors.popup_fg))
        ]),
        ratatui::text::Line::from(vec![
            changed_text,
            Span::styled(" : Modified code", Style::default().fg(app.colors.popup_fg))
        ]),
        ratatui::text::Line::from(vec![
            unchanged_text,
            Span::styled("  : Common code", Style::default().fg(app.colors.popup_fg))
        ]),
        ratatui::text::Line::from(""),
        ratatui::text::Line::from(format!("Theme: {}", match app.theme {
            Theme::Light => "Light",
            Theme::Dark => "Dark",
        })),
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

