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

//! [`RenderOptions`] and the display filter that applies them to painted ranges.

use crate::diff::text_range::{SourceText, TextRange};

/// One painted range: the `operation` for `source`, and the matching range on the other side.
/// Named source/destination rather than before/after because it serves both directions.
#[derive(Debug, Clone, PartialEq)]
pub struct RangeMatch {
    pub source: TextRange,
    pub destination: TextRange,
    pub operation: TextOperation,
}

impl RangeMatch {
    pub fn zero() -> Self {
        RangeMatch {
            source: TextRange::zero(),
            destination: TextRange::zero(),
            operation: TextOperation::NotYetSet,
        }
    }

    pub fn is_zero(&self) -> bool {
        self.source.is_zero()
            && self.destination.is_zero()
            && self.operation == TextOperation::NotYetSet
    }

    /// Whether `self` can merge into `other`: same operation, and only whitespace between them on
    /// both sides. Takes a prebuilt [`SourceText`] per side because turning positions into byte
    /// offsets by walking the file dominates on large files.
    pub fn extends(
        &self,
        other: &RangeMatch,
        source_code: &SourceText,
        dest_code: &SourceText,
    ) -> bool {
        if self.operation != other.operation {
            return false;
        }
        self.source
            .can_extend_with_whitespace(&other.source, source_code)
            && self
                .destination
                .can_extend_with_whitespace(&other.destination, dest_code)
    }

    pub fn extend_into(&mut self, other: &RangeMatch) {
        self.source.extend_to_end(&other.source);
        self.destination.extend_to_end(&other.destination);
    }
}

/// How a range renders. Not `ASTMappingOperation`: e.g. `InsertWithChildren` has no textual
/// meaning.
#[derive(Debug, Clone, Default, PartialEq)]
pub enum TextOperation {
    #[default]
    /// Sentinel value.
    NotYetSet,
    /// The ranges are identical.
    Identical,
    /// The range was moved somewhere else.
    Move,
    /// The text in the range differs.
    Update,
    /// The range was inserted.
    Insert,
    /// The range was deleted.
    Delete,
}

/// Which parts of a diff to paint. [`RenderOptions::MINIMAL`] and [`RenderOptions::FULL`] are
/// presets; each option can be toggled on its own. Turning an option on paints more, never less.
///
/// Trailing whitespace is not an option: no hand painting marks it, so `ranges_for_options` always
/// trims it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RenderOptions {
    /// Whether an `Insert`/`Delete` range keeps the indentation in front of it, on every row. Off,
    /// a multi-row range splits into one piece per row, each trimmed to its own content, since no
    /// single `TextRange` can trim every row's indentation. One field for the first row and the
    /// interior rows because no painting ever wants them to differ.
    ///
    /// `Move`/`Update` ranges are never split: their destinations are real positions, and a
    /// per-row split would need a matching split on the other side.
    pub leading_whitespace: bool,
    /// Whether a range of nothing but `STRUCTURAL_PUNCTUATION` is kept. Off in `MINIMAL`: the
    /// painted corpus drops lone brackets and separators.
    pub structural_punctuation: bool,
    /// Whether every updated matched pair is highlighted whole (`argument` -> `i_am_an_argument`
    /// entirely marked) instead of narrowed to the differing middle. Off in both presets, because
    /// the corpus was painted narrow either way; reached via `--whole-updates` or the `M` panel.
    ///
    /// Construction-time, like every option after it: it changes which ranges
    /// `TextDiff::from_with_options` builds, so changing it rebuilds the diff
    /// ([`Self::needs_rebuild_from`]).
    ///
    /// `#[serde(default)]` on every field added after `RenderOptions` joined the saved config: a
    /// missing key would otherwise fail the whole config load and reset the theme too.
    #[serde(default)]
    pub whole_pair_updates: bool,
    /// Whether a node tagged as a pure reindent (`solve_nested_condition_collapse`'s `if let`-chain
    /// collapse, `WrapGrowth`) still paints `Move`. The presets disagree, matching
    /// `rust-next-font-imports-generator`'s separate `Minimal` and `Full` paintings.
    ///
    /// Only for tagged nodes, never any column shift: position alone cannot tell a reindent from a
    /// real relocation (`rust-add-if`).
    ///
    /// Defaults to `true` when missing from a saved config, the behaviour before the field existed.
    #[serde(default = "paint_reindent_only_moves_default")]
    pub paint_reindent_only_moves: bool,
    /// Whether a node that kept its text and its place beside the edit on its row, and moved only
    /// because text was inserted or removed before it, still paints `Move`
    /// (`shellscript-ansible-ansible-a-small-add`). A displacement, unlike a reindent, leaves the
    /// row's content around the node unchanged, hence a separate field from
    /// [`Self::paint_reindent_only_moves`]. Read by `shifted_by_an_edit_beside_it` (single-row
    /// nodes) and `displaced_beside_an_edit_on_its_first_row` (multi-row).
    ///
    /// The presets disagree: `MINIMAL` paints as few bytes as it can, `FULL` keeps the displaced
    /// span painted with the construct around it. Defaults to `true` when missing from a saved
    /// config.
    #[serde(default = "paint_displaced_moves_default")]
    pub paint_displaced_moves: bool,
    /// Whether a relocation the two walks describe over different extents paints on both sides.
    /// On, `reconcile_moves` reads such a pair as agreement and keeps both claims; off, it keeps
    /// only the more economical account.
    ///
    /// The presets disagree: mirroring a relocation onto the second panel doubles the bytes
    /// `MINIMAL` paints, while `FULL` wants a `Move` that leads to a highlighted node.
    #[serde(default = "paint_resized_moves_default")]
    pub paint_resized_moves: bool,
    /// Whether a renamed identifier is highlighted whole on both sides instead of narrowed to the
    /// differing characters. On in `FULL` only, per ground-truth invariant 16
    /// (`identifier_updates_are_painted_by_preset`): `getOpt` -> `getKey` is a different name, not a
    /// kept `get`.
    ///
    /// Narrower than [`Self::whole_pair_updates`]: comments, strings and markup text keep their
    /// narrow reading, because a changed year in a copyright line is still one changed year.
    #[serde(default)]
    pub whole_identifier_updates: bool,
}

/// [`RenderOptions::paint_reindent_only_moves`]'s serde default.
pub(crate) fn paint_reindent_only_moves_default() -> bool {
    true
}

/// [`RenderOptions::paint_displaced_moves`]'s serde default.
pub(crate) fn paint_displaced_moves_default() -> bool {
    true
}

/// [`RenderOptions::paint_resized_moves`]'s serde default.
pub(crate) fn paint_resized_moves_default() -> bool {
    true
}

impl RenderOptions {
    /// Every option off: the tightest reading of a diff.
    pub const MINIMAL: Self = Self {
        leading_whitespace: false,
        structural_punctuation: false,
        whole_pair_updates: false,
        paint_reindent_only_moves: false,
        paint_displaced_moves: false,
        paint_resized_moves: false,
        whole_identifier_updates: false,
    };
    /// The fullest reading of a diff: every option on except `whole_pair_updates`, which is
    /// outside the preset axis.
    pub const FULL: Self = Self {
        leading_whitespace: true,
        structural_punctuation: true,
        whole_pair_updates: false,
        paint_reindent_only_moves: true,
        paint_displaced_moves: true,
        paint_resized_moves: true,
        whole_identifier_updates: true,
    };

    /// Every option with its label and value, in settings-UI order. Indexes match
    /// [`Self::toggle`].
    pub fn options(&self) -> [(&'static str, bool); 7] {
        [
            ("Leading whitespace", self.leading_whitespace),
            (
                "Structural punctuation (brackets, separators)",
                self.structural_punctuation,
            ),
            ("Whole-pair updates", self.whole_pair_updates),
            ("Paint reindent-only moves", self.paint_reindent_only_moves),
            ("Paint displaced moves", self.paint_displaced_moves),
            (
                "Paint moves the two sides size differently",
                self.paint_resized_moves,
            ),
            ("Whole renamed identifiers", self.whole_identifier_updates),
        ]
    }

    /// Flips the option at [`Self::options`]'s index `i`; out of range is a no-op, not a panic.
    pub fn toggle(&mut self, i: usize) {
        match i {
            0 => self.leading_whitespace = !self.leading_whitespace,
            1 => self.structural_punctuation = !self.structural_punctuation,
            2 => self.whole_pair_updates = !self.whole_pair_updates,
            3 => self.paint_reindent_only_moves = !self.paint_reindent_only_moves,
            4 => self.paint_displaced_moves = !self.paint_displaced_moves,
            5 => self.paint_resized_moves = !self.paint_resized_moves,
            6 => self.whole_identifier_updates = !self.whole_identifier_updates,
            _ => {}
        }
    }

    /// Whether going from `previous` to these options changes which ranges the diff is built
    /// with, so a viewer has to rebuild it rather than re-filter the ranges it already has.
    ///
    /// Stated by exclusion, naming only the post-filters `ranges_for_options` applies, so a new
    /// field rebuilds by default: a needless rebuild costs time, a missing one shows a stale diff.
    pub fn needs_rebuild_from(&self, previous: &Self) -> bool {
        let post_filters_only = Self {
            leading_whitespace: previous.leading_whitespace,
            structural_punctuation: previous.structural_punctuation,
            ..*self
        };
        post_filters_only != *previous
    }
}

impl Default for RenderOptions {
    /// The fullest rendering, so a config or script that never mentions these options gets
    /// everything painted rather than nothing.
    fn default() -> Self {
        Self::FULL
    }
}

/// The characters [`RenderOptions::structural_punctuation`] treats as structural. Operators are
/// absent on purpose: `<` to `<=` is the whole edit, and dropping it reports a different diff.
pub(crate) const STRUCTURAL_PUNCTUATION: &[char] = &['(', ')', '[', ']', '{', '}', ',', ';', ':'];

/// Whether `text` is nothing but structural punctuation and whitespace. Empty text is not: a
/// zero-width range is the only mark one side has of what the other side gained or lost.
pub fn is_structural_only(text: &str) -> bool {
    !text.is_empty()
        && text
            .chars()
            .all(|c| c.is_whitespace() || STRUCTURAL_PUNCTUATION.contains(&c))
}

/// One side's ranges as `options` would paint them. Trailing whitespace is always trimmed.
///
/// A surviving range describes the same edit under every option, only padded differently. The one
/// exception is the per-row split of [`RenderOptions::leading_whitespace`].
pub fn ranges_for_options(
    ranges: &[RangeMatch],
    source: &str,
    options: RenderOptions,
) -> Vec<RangeMatch> {
    let result = ranges_for_options_impl(ranges, source, options);
    if options.structural_punctuation {
        return result;
    }
    restore_paired_brackets(ranges, source, options, result)
}

pub(crate) fn ranges_for_options_impl(
    ranges: &[RangeMatch],
    source: &str,
    options: RenderOptions,
) -> Vec<RangeMatch> {
    let lines: Vec<&str> = source.split('\n').collect();

    // Rows holding matched (non-`Insert`/`Delete`) content, whose indentation an `Insert`/`Delete`
    // must not grow over.
    let mut matched_rows = std::collections::HashSet::new();
    for range_match in ranges {
        if !matches!(
            range_match.operation,
            TextOperation::Insert | TextOperation::Delete
        ) {
            let source = &range_match.source;
            // An end at column 0 excludes that row, so a newline range must not veto the wholly
            // new line after it (`rust-hello-world-added-message`).
            let last_touched_row = if source.end_column == 0 && source.end_row > source.start_row {
                source.end_row - 1
            } else {
                source.end_row
            };
            matched_rows.extend(source.start_row..=last_touched_row);
        }
    }

    ranges
        .iter()
        .flat_map(|range_match| {
            // Kept although unpainted: `line_operations` colours rows from them.
            if range_match.operation == TextOperation::Identical
                || range_match.operation == TextOperation::NotYetSet
            {
                return vec![range_match.clone()];
            }

            if !options.leading_whitespace
                && matches!(
                    range_match.operation,
                    TextOperation::Insert | TextOperation::Delete
                )
                && range_match.source.start_row != range_match.source.end_row
            {
                // Skips `narrow_one_range`, whose leading-whitespace extension would grow a piece
                // back over the indentation this split drops.
                return split_into_per_row_pieces(&lines, range_match)
                    .into_iter()
                    .filter(|piece| {
                        options.structural_punctuation
                            || range_is_structural_only(&lines, &piece.source) != Some(true)
                    })
                    .collect();
            }

            narrow_one_range(&lines, &matched_rows, options, range_match)
                .into_iter()
                .collect()
        })
        .collect()
}

/// Restores a range dropped as standalone punctuation when its bracket's partner was painted, so
/// a reader never sees half a pair. Merging can bundle `(` into `max_val = max(` while `)` lands
/// alone (`python-refactoring`); a pair whose halves were both dropped stays dropped.
///
/// Brackets pair by a plain nesting scan, not a lexer, so a bracket inside a string or comment can
/// misalign the rest of the file. Accepted for a rendering-only heuristic.
pub(crate) fn restore_paired_brackets(
    ranges: &[RangeMatch],
    source: &str,
    options: RenderOptions,
    mut result: Vec<RangeMatch>,
) -> Vec<RangeMatch> {
    let with_structural_kept = ranges_for_options_impl(
        ranges,
        source,
        RenderOptions {
            structural_punctuation: true,
            ..options
        },
    );
    let dropped: Vec<&RangeMatch> = with_structural_kept
        .iter()
        .filter(|candidate| {
            !result.contains(candidate)
                // A lone `Move`/`Update` bracket is column-shift noise, not a pairing question
                // (`javascript-refactor-arrow-func`).
                && matches!(candidate.operation, TextOperation::Insert | TextOperation::Delete)
        })
        .collect();
    if dropped.is_empty() {
        return result;
    }

    let text = SourceText::new(source);
    // `None` for an unaddressable position, so such a range covers nothing.
    let byte_range = |range: &TextRange| -> Option<(usize, usize)> {
        let start = text.byte_index(
            crate::diff::text_range::SourceRow::from_raw(range.start_row),
            crate::diff::text_range::SourceColumn::from_raw(range.start_column),
        )?;
        let end = text.byte_index(
            crate::diff::text_range::SourceRow::from_raw(range.end_row),
            crate::diff::text_range::SourceColumn::from_raw(range.end_column),
        )?;
        Some((start.get(), end.get()))
    };
    let partners = bracket_pair_partners(source);
    let covered = |byte: usize| {
        result
            .iter()
            .any(|kept| byte_range(&kept.source).is_some_and(|(s, e)| s <= byte && byte < e))
    };

    let mut restored = Vec::new();
    for candidate in dropped {
        let Some((start, end)) = byte_range(&candidate.source) else {
            continue;
        };
        let Some(text) = source.get(start..end) else {
            continue;
        };
        let has_a_surviving_partner = text
            .char_indices()
            .filter(|&(_, c)| matches!(c, '(' | ')' | '[' | ']' | '{' | '}'))
            .any(|(i, _)| {
                partners
                    .get(&(start + i))
                    .is_some_and(|&partner_byte| covered(partner_byte))
            });
        if has_a_surviving_partner {
            restored.push((*candidate).clone());
        }
    }
    result.extend(restored);
    result
}

/// Each bracket's byte position mapped to its partner's, by a plain nesting scan (see
/// [`restore_paired_brackets`]). A mismatched or unbalanced bracket gets no entry.
pub(crate) fn bracket_pair_partners(source: &str) -> std::collections::HashMap<usize, usize> {
    let mut partners = std::collections::HashMap::new();
    let mut stack: Vec<(char, usize)> = Vec::new();
    for (byte, ch) in source.char_indices() {
        match ch {
            '(' | '[' | '{' => stack.push((ch, byte)),
            ')' | ']' | '}' => {
                if let Some(&(open_ch, open_byte)) = stack.last()
                    && matches!((open_ch, ch), ('(', ')') | ('[', ']') | ('{', '}'))
                {
                    stack.pop();
                    partners.insert(open_byte, byte);
                    partners.insert(byte, open_byte);
                }
            }
            _ => {}
        }
    }
    partners
}

/// One range under `options`, except the per-row split: the structural-punctuation filter, the
/// trailing trim, then the leading trim or extension.
pub(crate) fn narrow_one_range(
    lines: &[&str],
    matched_rows: &std::collections::HashSet<usize>,
    options: RenderOptions,
    range_match: &RangeMatch,
) -> Option<RangeMatch> {
    if !options.structural_punctuation {
        match range_is_structural_only(lines, &range_match.source) {
            Some(true) => return None,
            Some(false) => {}
            // Unreadable: a display filter must not drop what it cannot interpret.
            None => return Some(range_match.clone()),
        }
    }
    let mut trimmed = range_match.clone();
    trimmed.source = trim_trailing_whitespace(lines, &range_match.source)?;
    if options.leading_whitespace {
        if matches!(
            range_match.operation,
            TextOperation::Insert | TextOperation::Delete
        ) && !matched_rows.contains(&trimmed.source.start_row)
        {
            trimmed.source = extend_leading_whitespace(lines, &trimmed.source);
        }
    } else {
        trimmed.source = trim_leading_whitespace(lines, &trimmed.source)?;
    }
    Some(trimmed)
}

/// A multi-row `Insert`/`Delete` as one piece per row, each trimmed to its own content; a blank row
/// gives no piece. Every piece keeps the range's destination, which for `Insert`/`Delete` is a
/// placeholder anchor, not a per-row position.
pub(crate) fn split_into_per_row_pieces(
    lines: &[&str],
    range_match: &RangeMatch,
) -> Vec<RangeMatch> {
    let source = &range_match.source;
    (source.start_row..=source.end_row)
        .filter_map(|row| {
            let line = *lines.get(row)?;
            let start_column = if row == source.start_row {
                source.start_column
            } else {
                0
            }
            .min(line.len());
            let end_column = if row == source.end_row {
                source.end_column
            } else {
                line.len()
            }
            .min(line.len());
            let row_range = TextRange::new(row, start_column, row, end_column);
            let row_range = trim_trailing_whitespace(lines, &row_range)?;
            let row_range = trim_leading_whitespace(lines, &row_range)?;
            Some(RangeMatch {
                source: row_range,
                destination: range_match.destination.clone(),
                operation: range_match.operation.clone(),
            })
        })
        .collect()
}

/// `range` with trailing whitespace (including newlines) removed, or `None` if only whitespace
/// remains. Narrows `source` only; see [`trim_leading_whitespace`].
pub(crate) fn trim_trailing_whitespace(lines: &[&str], range: &TextRange) -> Option<TextRange> {
    let (start_row, start_column) = (range.start_row, range.start_column);
    let (mut end_row, mut end_column) = (range.end_row, range.end_column);

    loop {
        if (start_row, start_column) >= (end_row, end_column) {
            return None;
        }
        // A range through the end of a file with no final newline ends one row past the last
        // line; step back rather than drop the whole range (`python-api-change`).
        if end_row >= lines.len() {
            end_row = end_row.checked_sub(1)?;
            end_column = lines.get(end_row)?.len();
            continue;
        }
        let line = *lines.get(end_row)?;
        let column = end_column.min(line.len());
        match line[..column].chars().next_back() {
            Some(c) if c.is_whitespace() => end_column = column - c.len_utf8(),
            // At column 0 the previous character is the previous row's newline.
            None => {
                end_row = end_row.checked_sub(1)?;
                end_column = lines.get(end_row)?.len();
            }
            Some(_) => break,
        }
    }
    Some(TextRange::new(start_row, start_column, end_row, end_column))
}

/// `range` with leading whitespace (including newlines) removed, or `None` if only whitespace
/// remains.
///
/// Narrows `source` only: `destination` is a position in the other file, whose text this cannot
/// see, and cross-panel navigation jumps to it.
pub(crate) fn trim_leading_whitespace(lines: &[&str], range: &TextRange) -> Option<TextRange> {
    let (mut start_row, mut start_column) = (range.start_row, range.start_column);
    let (end_row, end_column) = (range.end_row, range.end_column);

    loop {
        if (start_row, start_column) >= (end_row, end_column) {
            return None;
        }
        let line = *lines.get(start_row)?;
        match line[start_column.min(line.len())..].chars().next() {
            Some(c) if c.is_whitespace() => start_column += c.len_utf8(),
            // Past the row's end: step over its newline.
            None => {
                start_row += 1;
                start_column = 0;
            }
            Some(_) => break,
        }
    }
    Some(TextRange::new(start_row, start_column, end_row, end_column))
}

/// `range` grown back to column 0 when only whitespace precedes it on its start row. A node's
/// range never includes its indentation, and the corpus paints a whole new line's indentation as
/// part of the insertion (`rust-add-value-to-enum`).
///
/// Whitespace alone cannot say whether the indentation belongs to matched content sharing the row
/// (`const ` inserted before a moved declaration, `cpp-add-const-correctness`), so the caller vetoes
/// rows holding matched ranges. Grows `source` only.
pub(crate) fn extend_leading_whitespace(lines: &[&str], range: &TextRange) -> TextRange {
    let Some(line) = lines.get(range.start_row) else {
        return range.clone();
    };
    let prefix_end = range.start_column.min(line.len());
    // A char boundary: a node start or a start `trim_trailing_whitespace` left untouched.
    if line[..prefix_end].chars().all(char::is_whitespace) {
        TextRange::new(range.start_row, 0, range.end_row, range.end_column)
    } else {
        range.clone()
    }
}

/// Whether every character `range` covers is structural punctuation or whitespace. `None` if the
/// range falls outside the source.
pub(crate) fn range_is_structural_only(lines: &[&str], range: &TextRange) -> Option<bool> {
    if range.start_row > range.end_row {
        return None;
    }
    let mut saw_any = false;
    for row in range.start_row..=range.end_row {
        let line = *lines.get(row)?;
        let start = if row == range.start_row {
            range.start_column
        } else {
            0
        };
        let end = if row == range.end_row {
            range.end_column
        } else {
            line.len()
        };
        if start > line.len() || end > line.len() || start > end {
            return None;
        }
        let covered = line.get(start..end)?;
        if !covered.is_empty() {
            saw_any = true;
            if !is_structural_only(covered) {
                return Some(false);
            }
        }
        // The newline between rows is whitespace, so an all-blank multi-row range is structural.
        if row < range.end_row {
            saw_any = true;
        }
    }
    // A zero-width placeholder is not structural (see `is_structural_only`).
    Some(saw_any)
}
