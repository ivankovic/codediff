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
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Wrap},
};

use crate::tui::app::LineDiffStatus;

/// Code panel widget
pub struct CodePanel {
    pub title: String,
    pub code: String,
    pub line_number: usize,
    pub cursor_line: usize,
    pub cursor_char: usize,
    pub scroll_offset: usize,
    pub is_active: bool,
    pub colors: super::super::app::ThemeColors,
    pub token_diff_ranges: Vec<(usize, usize, LineDiffStatus)>
}

impl CodePanel {
    pub fn new(
        title: String,
        code: String,
        line_number: usize,
        cursor_line: usize,
        cursor_char: usize,
        scroll_offset: usize,
        is_active: bool,
        colors: super::super::app::ThemeColors,
        token_diff_ranges: Vec<(usize, usize, LineDiffStatus)>
    ) -> Self {
        Self {
            title,
            code,
            line_number,
            cursor_line,
            cursor_char,
            scroll_offset,
            is_active,
            colors,
            token_diff_ranges
        }
    }

    pub fn to_widget(&self, visible_lines: usize) -> Paragraph<'static> {
        let lines: Vec<Line> = self.code
            .lines()
            .enumerate()
            .skip(self.scroll_offset)
            .take(visible_lines)
            .map(|(line_idx, line)| {
                let mut spans = Vec::new();

                // Add line number prefix
                spans.push(Span::styled(
                    format!("{:4} ", self.line_number + line_idx + 1),
                    Style::default().fg(self.colors.footer_fg),
                ));

                // Calculate byte position for this line
                let line_start_byte = self.code[..self.code
                    .lines()
                    .take(line_idx)
                    .map(|l| l.len() + 1)
                    .sum::<usize>()
                    .saturating_sub(1)]
                    .len();

                // Apply token-based coloring
                let is_active_line = self.is_active && line_idx == self.cursor_line;
                
                // Find token ranges that overlap with this line
                let line_ranges: Vec<_> = self
                    .token_diff_ranges
                    .iter()
                    .filter(|&&(start, end, _)| {
                        start < line_start_byte + line.len() && end > line_start_byte
                    })
                    .collect();

                if is_active_line {
                    for (byte_idx, c) in line.char_indices() {
                        let char_byte_pos = line_start_byte + byte_idx;

                        if byte_idx == self.cursor_char {
                            // Highlight current character
                            spans.push(Span::styled(
                                c.to_string(),
                                Style::default()
                                    .fg(self.colors.cursor_fg)
                                    .bg(self.colors.cursor_bg)
                                    .add_modifier(Modifier::BOLD),
                            ));
                        } else {
                            // Find color based on token ranges
                            let mut char_color = self.colors.text;
                            for (start, end, status) in &line_ranges {
                                if char_byte_pos >= *start && char_byte_pos < *end {
                                    char_color = match status {
                                        LineDiffStatus::Added => self.colors.diff_added,
                                        LineDiffStatus::Removed => self.colors.diff_removed,
                                        LineDiffStatus::Changed => self.colors.diff_changed,
                                        LineDiffStatus::Unchanged => self.colors.text,
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
                        let mut char_color = self.colors.text;
                        for (start, end, status) in &line_ranges {
                            if char_byte_pos >= *start && char_byte_pos < *end {
                                char_color = match status {
                                    LineDiffStatus::Added => self.colors.diff_added,
                                    LineDiffStatus::Removed => self.colors.diff_removed,
                                    LineDiffStatus::Changed => self.colors.diff_changed,
                                    LineDiffStatus::Unchanged => self.colors.text,
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

        Paragraph::new(Text::from(lines))
            .block(Block::default().borders(Borders::ALL).title(self.title.clone()))
            .wrap(Wrap { trim: true })
    }
}