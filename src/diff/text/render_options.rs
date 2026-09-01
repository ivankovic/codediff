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

// Split out of text.rs (its `RenderOptions` + the post-AST filtering/painting logic that
// consumes ranges and applies user-configured display options) purely to shrink that file's
// visible size. No behavior change.

use crate::diff::text_range::{SourceText, TextRange};

/**
* A textual range match. For a given source match, it provides the operation for that range and
* optionally the matching range on the destination side.
*
* Note that it doesn't use before or after terms on purpose, because it is used for both
* before-to-after and after-to-before ranges.
*/
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

    /// Takes a [`SourceText`] per side rather than a `&str`, because deciding this needs to look
    /// at the text *between* two ranges, which means turning row/column positions into byte
    /// offsets - and doing that by walking the file was 90% of the corpus's worst fixture. See
    /// `SourceText`'s own doc comment for the measurement. Callers build one per side per call to
    /// `ranges`, not one per comparison.
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

/**
* The diff operation.
*
* Why not re-use ASTMappingOperation struct? It's not a 1:1 match. For example "InsertWithChildren"
* is not a valid textual operation.
*/
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

/// Which parts of a diff to actually paint.
///
/// Not a difference in what was computed - the mapping is identical either way - but in how much
/// of it is worth showing. Every option is independent and additive: turning one on paints more,
/// never less. [`RenderOptions::MINIMAL`]/[`RenderOptions::FULL`] are the two extremes, not the
/// only two states a reader can be in - each option is meant to be toggled on its own.
///
/// **Trailing whitespace is deliberately not an option here.** Nobody painting a diff by hand ever
/// marks a line's trailing whitespace, and least of all the newline past it, so
/// `ranges_for_options` trims it unconditionally - under every combination of options, including
/// `FULL`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RenderOptions {
    /// Whether a range that starts in whitespace keeps it, or has it trimmed back to the first
    /// real character - applied to *every* row of a multi-row `Insert`/`Delete`, not just the
    /// range's own first one. Off in `MINIMAL`: painting by hand, nobody marks the indentation
    /// before a change as part of it, on any of its lines. On in `FULL`, which also keeps a
    /// multi-row range as one contiguous span rather than splitting it per row.
    ///
    /// **Governs both the rules doc's choice-3/4 axis (this range's own leading edge) and its
    /// choice-2 axis (every interior row's own leading edge) - one field, not two.** Until
    /// 2026-09-01 these were separate fields (`leading_whitespace` for the first row,
    /// `interior_line_indentation` for the rest), and unchecking only the first while the corpus's
    /// own `MINIMAL`/`FULL` ground truth always wanted both moving together (measured: flipping
    /// `MINIMAL`'s then-separate `interior_line_indentation` to `false` alone dropped the handmade
    /// aggregate from 1.2590% to 1.1811%, ~40 fixtures improved, one unrelated regression - see
    /// `painting_disagreement_fix_log.md`) was exactly the confusing, redundant-in-practice split a
    /// user working the `M` panel by hand ran into directly: unchecking "leading whitespace" left
    /// every row but the range's own first one still fully indented, because the two fields were
    /// never actually independent in any painting the corpus holds. Merged into this one field
    /// rather than kept as two that happen to always agree.
    ///
    /// **Off means split.** When this is `false`, a multi-row `Insert`/`Delete` is rendered as one
    /// piece per row, each trimmed to its own real content, leading *and* trailing (see
    /// `split_into_per_row_pieces`) - there is no single `TextRange` that can describe "every row's
    /// own indentation trimmed independently" (a contiguous range's interior rows are always
    /// `0..row_len` wherever this crate reads one, e.g. `TextRange::columns_on_row`), so
    /// representing this reading at all requires more than one `RangeMatch` for what was one edit.
    ///
    /// Only meaningful for a range that has *some* real character to trim back to - a range that
    /// is nothing but whitespace start to end is dropped by the unconditional trailing-whitespace
    /// trim regardless of this option, the same as it always has been under `structural_punctuation`
    /// (see `range_is_structural_only`, which already treats an all-whitespace range as structural).
    ///
    /// **Scoped to `Insert`/`Delete`, same as before the merge.** A `Move`/`Update` range spanning
    /// several rows is a *matched* pair, whose `destination` is a real cross-file position rather
    /// than `Insert`/`Delete`'s placeholder anchor (see `advance_and_build_range`'s doc comment) -
    /// splitting it into a source-side piece per row would need a matching destination-side split,
    /// which needs the two sides' row layouts to correspond and isn't implemented. `Update`'s own
    /// indentation rule is a different, narrower one the rules doc states separately ("inserted or
    /// deleted whitespace... highlighted at the start of the line") that `intra_node_update_ranges`
    /// already follows.
    pub leading_whitespace: bool,
    /// Whether a range consisting solely of structural punctuation (see [`STRUCTURAL_PUNCTUATION`])
    /// is kept or dropped. Off in `MINIMAL` - measured against the corpus's hand-painted ground
    /// truth: across the ten fixtures painted both ways, keeping these carries 27 entries dropping
    /// them does not, and **16 of those 27 are a single punctuation token** - eight `(`, five `)`,
    /// two `):` and one `;`. On in `FULL`.
    pub structural_punctuation: bool,
    /// Whether an `Update` node's own matched pair is highlighted whole (`argument` and
    /// `i_am_an_argument` both entirely marked as the changed region), or - the default, and the
    /// only reading the corpus's own hand-painted ground truth was ever painted against - narrowed
    /// to just the common-prefix/common-suffix-trimmed middle that actually differs.
    ///
    /// **Not a third state of `MINIMAL`/`FULL`, deliberately.** Unlike `leading_whitespace`/
    /// `structural_punctuation`, this doesn't change how much of an *already-computed* range list
    /// gets painted - it changes which ranges `TextDiff::from_with_update_style` builds in the
    /// first place (see `intra_node_update_ranges`), so it can't be applied by
    /// `ranges_for_options` as a pure post-filter the way the other two are. `MINIMAL`/`FULL` both
    /// leave it `false`: the corpus was painted narrow either way, so turning this on under either
    /// preset would disagree with ground truth that was never asked about this axis. Reached only
    /// via `--whole-updates` (see `main.rs`) for batch mode. Also toggleable live in the `M`
    /// panel - unlike the two fields above, flipping it there triggers a full diff reload rather
    /// than `DiffViewer::set_render_options`'s plain re-filter, since a re-filter alone can't
    /// reach a field that changes which ranges get built in the first place - see
    /// `tui::app::App::start_diff` (reads this off the live `DiffViewer` for every diff, not just
    /// the ones this option's own toggle triggers) and its `Action::RenderOptionsChanged` handler
    /// (the one place that notices the field changed and asks for a reload).
    ///
    /// `#[serde(default)]`, unlike the two fields above: this one arrived after `RenderOptions`
    /// was already a field of `theme::ThemeConfig` and so already round-tripping through an
    /// existing `.codediff.toml` on disk. Without a default, `confy`'s deserialization of that
    /// pre-existing file would fail on the missing key, and `theme::load_from`'s `.unwrap_or_default()`
    /// would silently reset not just this option but the *entire* config - theme and syntax theme
    /// included - back to defaults on the next read. The default (`false`, i.e. narrow) is exactly
    /// what an old config without an opinion on this axis should mean anyway.
    #[serde(default)]
    pub whole_pair_updates: bool,
    /// Whether a matched node that relocated *purely* because nesting levels were added or
    /// removed around it (e.g. Rust's `if let`-chain collapse - `solve_nested_condition_collapse`
    /// tags its `BODY` match this way) still paints as `Move`, or is left unpainted as
    /// effectively unchanged.
    ///
    /// **A real axis, unlike `whole_pair_updates`.** `MINIMAL` and `FULL` disagree here, not just
    /// leave it off: measured directly against `rust-next-font-imports-generator`'s hand-painted
    /// ground truth, which carries *separate* `Minimal`/`Full` paintings that disagree on exactly
    /// this - `Minimal` wants the reindented body unpainted (37.788% -> 6.537% disagreement),
    /// `Full` wants it painted `Move` (22.212% -> 49.809% if suppressed). So `MINIMAL` sets this
    /// `false`, `FULL` sets it `true`, matching each preset's own painting exactly. See the
    /// 2026-09-01 painting-baseline investigation for the measurement.
    ///
    /// **Deliberately narrow, not a blanket column-shift heuristic.** `ranges` cannot tell a
    /// node that relocated by pure reindent from one that genuinely moved to a new position by
    /// column position alone - both look identical (see `rust-add-if`, a block that really did
    /// move, cited in `ranges`'s own doc comment on `column_shift_is_meaningful`). Suppressing
    /// `Move` for *every* column-shifted node when this is `false` would silently regress that
    /// fixture back to the 56.5% disagreement a past measurement already ruled out. This option
    /// only ever applies to a node `solve_nested_condition_collapse` has already verified,
    /// structurally, is a pure reindent - see `ranges`'s `known_pure_reindent` check.
    ///
    /// **Construction-time, like `whole_pair_updates`.** It decides `TextOperation::Move` vs.
    /// `Identical` while `ranges` builds its range list, not a post-filter `ranges_for_options`
    /// could apply afterward - flipping it in the `M` panel triggers a full diff reload for the
    /// same reason `whole_pair_updates` does (see `tui::app::App::apply_render_options`).
    ///
    /// `#[serde(default = "paint_reindent_only_moves_default")]`: an existing `.codediff.toml`
    /// predates this field and this pass entirely, so the only sound default is the behavior
    /// every prior release had - always paint the `Move` (`true`) - not `bool::default()`'s
    /// `false`, which would be the wrong polarity - the same reasoning `leading_whitespace`'s own
    /// doc comment gives for why merging its old sibling field in didn't need a matching change
    /// here (that field already had a non-derived default before the merge).
    #[serde(default = "paint_reindent_only_moves_default")]
    pub paint_reindent_only_moves: bool,
}

/// [`RenderOptions::paint_reindent_only_moves`]'s serde default - see that field's own doc
/// comment.
pub(crate) fn paint_reindent_only_moves_default() -> bool {
    true
}

impl RenderOptions {
    /// Every option off: the tightest reading of a diff - drops standalone punctuation and trims
    /// leading whitespace off what remains, row by row inside a multi-row insert/delete too (see
    /// `leading_whitespace`'s own doc comment for the corpus measurement behind that as of
    /// 2026-09-01). Trailing whitespace is already always trimmed regardless of any option - see
    /// the struct's own doc comment.
    pub const MINIMAL: Self = Self {
        leading_whitespace: false,
        structural_punctuation: false,
        whole_pair_updates: false,
        paint_reindent_only_moves: false,
    };
    /// Every option on: the fullest reading of a diff, short of trailing whitespace, which no
    /// combination of options ever paints. `whole_pair_updates` stays off even here - see that
    /// field's own doc comment for why it isn't part of this axis at all.
    pub const FULL: Self = Self {
        leading_whitespace: true,
        structural_punctuation: true,
        whole_pair_updates: false,
        paint_reindent_only_moves: true,
    };

    /// Every option, paired with its label and current value, in the order a settings UI should
    /// list them. A future option means one new field above and one new entry here - nothing that
    /// reads this array (the settings dialog, in particular) needs to change.
    ///
    /// `whole_pair_updates` belongs here despite its own doc comment explaining it isn't part of
    /// the `MINIMAL`/`FULL` *axis* - that's a statement about what the two presets set, not about
    /// whether a settings UI can offer it. The `M` panel toggling this one now goes through a
    /// diff reload rather than `DiffViewer::set_render_options`'s plain re-filter (see
    /// `tui::app`'s `Action::RenderOptionsChanged` handler) precisely so it's safe to list here.
    pub fn options(&self) -> [(&'static str, bool); 4] {
        [
            ("Leading whitespace", self.leading_whitespace),
            (
                "Structural punctuation (brackets, separators)",
                self.structural_punctuation,
            ),
            ("Whole-pair updates", self.whole_pair_updates),
            ("Paint reindent-only moves", self.paint_reindent_only_moves),
        ]
    }

    /// Flips the option at [`Self::options`]'s index `i`. A no-op if `i` is out of range, rather
    /// than a panic: a settings UI driving this from a row index it computed itself has no way to
    /// pass one out of range, but nothing here should crash the process if it somehow did.
    pub fn toggle(&mut self, i: usize) {
        match i {
            0 => self.leading_whitespace = !self.leading_whitespace,
            1 => self.structural_punctuation = !self.structural_punctuation,
            2 => self.whole_pair_updates = !self.whole_pair_updates,
            3 => self.paint_reindent_only_moves = !self.paint_reindent_only_moves,
            _ => {}
        }
    }
}

impl Default for RenderOptions {
    /// Matches every release before this setting existed, and `RenderMode`'s own prior default:
    /// an existing config or script that never mentions this setting keeps behaving exactly as it
    /// did.
    fn default() -> Self {
        Self::FULL
    }
}

/// The characters [`RenderOptions::structural_punctuation`] treats as structural - brackets,
/// separators and whitespace.
///
/// **Operators are deliberately absent.** `+`, `=`, `<`, `&&` are punctuation to a tokenizer but
/// carry the entire meaning of a change to a reader: an edit from `<` to `<=` is the whole edit,
/// and dropping it would be reporting a different diff rather than a tighter one. Every token this
/// actually drops was observed being dropped by hand in the painted corpus; nothing here is
/// included on the grounds that it merely looks like punctuation.
pub(crate) const STRUCTURAL_PUNCTUATION: &[char] = &['(', ')', '[', ']', '{', '}', ',', ';', ':'];

/// Whether `text` is nothing but structural punctuation and whitespace - i.e. whether a range
/// covering it says anything a reader needs when [`RenderOptions::structural_punctuation`] is off.
///
/// Empty text is *not* structural: a zero-width range is an insert/delete placeholder marking a
/// position, and dropping those would remove the only mark one side has for what the other side
/// gained or lost.
pub fn is_structural_only(text: &str) -> bool {
    !text.is_empty()
        && text
            .chars()
            .all(|c| c.is_whitespace() || STRUCTURAL_PUNCTUATION.contains(&c))
}

/// One side's ranges as `options` would paint them.
///
/// Trailing whitespace is trimmed off every surviving range's end unconditionally - see
/// [`RenderOptions`]'s own doc comment for why that is not itself an option. Leading whitespace and
/// standalone-structural-punctuation ranges are each independently controlled by `options`;
/// [`RenderOptions::MINIMAL`]/[`RenderOptions::FULL`] are exactly "every option off"/"every option
/// on", so between them they reproduce this function's whole behavior range.
///
/// A range is never merged or re-operated - a surviving range still describes the same edit every
/// other combination of options describes, just with different padding, or absent entirely once
/// nothing else is left in it. **Split is the one exception**: a multi-row `Insert`/`Delete` under
/// `leading_whitespace: false` becomes one range per row (see
/// `split_into_per_row_pieces`) - there is no single `TextRange` that can describe "every row's own
/// indentation trimmed independently" (a contiguous range's interior rows are always `0..row_len`
/// wherever this crate reads one, e.g. `TextRange::columns_on_row`), so representing that choice at
/// all requires more than one `RangeMatch` for what was one edit.
pub fn ranges_for_options(
    ranges: &[RangeMatch],
    source: &str,
    options: RenderOptions,
) -> Vec<RangeMatch> {
    let result = ranges_for_options_impl(ranges, source, options);
    if options.structural_punctuation {
        // Nothing was ever dropped as standalone punctuation - the restoration pass below has
        // nothing to do, and skipping it avoids computing `with_structural_kept` (a second full
        // filter pass) for the common case that doesn't need it.
        return result;
    }
    restore_paired_brackets(ranges, source, options, result)
}

pub(crate) fn ranges_for_options_impl(
    ranges: &[RangeMatch],
    source: &str,
    options: RenderOptions,
) -> Vec<RangeMatch> {
    // Built once for the whole call rather than rescanning the file per range: `range_text` walked
    // from the top of the source for every range it was asked about, which on a large file with
    // many ranges is the dominant cost of painting a frame.
    let lines: Vec<&str> = source.split('\n').collect();

    // Rows carrying some Identical/Update/Move content, i.e. content matched to (or from) the
    // other file - built once and consulted by `extend_leading_whitespace`, which must never grow
    // an Insert/Delete range backward into a row that also holds one of these: that whitespace
    // belongs to the matched content sharing the row, not to the inserted/deleted piece. A row
    // touched only by Insert/Delete (however many separate ones) carries nothing else to protect.
    let mut matched_rows = std::collections::HashSet::new();
    for range_match in ranges {
        if !matches!(
            range_match.operation,
            TextOperation::Insert | TextOperation::Delete
        ) {
            let source = &range_match.source;
            // A range ending exactly at column 0 of a later row (the common shape for "matched
            // content runs through this row's own trailing newline") stops there - the end is
            // exclusive, so it never actually touches that row's own content, and must not mark it
            // as matched. Without this, a `RangeMatch` for the newline right before a wholly new
            // line (`rust-hello-world-added-message`'s own shape) would wrongly veto that line's
            // own `extend_leading_whitespace` call.
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
            // An `Identical` range is the unpainted background in every consumer, so whether it
            // survives changes nothing on screen - but keeping it keeps every combination of
            // options' range lists structurally comparable, and `line_operations` relies on the
            // identical ranges being present to colour a row it would otherwise leave blank.
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
                // Choice 2: each piece already came out trimmed to its own row's real content
                // (leading and trailing both - see `split_into_per_row_pieces`), so only the
                // structural-punctuation filter still applies. Re-running the trailing/leading
                // trim below would be a harmless no-op for the trim, but `leading_whitespace`'s
                // extension would grow a row straight back over the indentation this branch
                // exists to drop - so this shape skips that whole path rather than merely
                // happening to survive it.
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

/// Un-drops any range `ranges_for_options_impl` filtered out purely for being standalone
/// structural punctuation, if painting it survived on its *own* character but its matching
/// bracket partner did not - so a reader never sees one half of a `(...)`/`[...]`/`{...}` pair
/// without the other.
///
/// **Why this exists.** `range_is_structural_only` judges one `RangeMatch` in isolation, but the
/// diff's own range-merging (`RangeMatch::extends`) can bundle one bracket into a bigger range
/// with real content next to it (`"max_val = max("`, kept - it isn't pure punctuation) while its
/// partner ends up alone in its own range (`")"`, dropped - it is) - not because the two brackets
/// disagree about anything, but because of where the *other*, unrelated content around each one
/// happened to fall. Confirmed on `python-refactoring`: `max_val = max(numbers)` painted the `(`
/// (bundled with `max`) but silently dropped the `)` (alone in its own range) under `MINIMAL`.
///
/// **How.** Recomputes the same filter with `structural_punctuation` forced on (the version that
/// never drops anything for this reason) and diffs the two outputs to find exactly what the real
/// call dropped. For each dropped range that is - or starts/ends on - a bracket character, looks
/// up that bracket's partner via [`bracket_pair_partners`] and restores the range only if the
/// partner's position is covered by a range the real (requested) output already kept - so a pair
/// where *both* halves would otherwise be dropped stays dropped, matching `structural_punctuation:
/// false`'s own reading for an ordinary standalone pair with nothing else painted nearby.
///
/// **Bracket pairing is a plain nesting-depth scan over raw text, not lexer-aware** - a bracket
/// character sitting inside a string literal or comment can desync the rest of the scan for that
/// file. Accepted: this is a rendering-only heuristic (which range gets a few extra punctuation
/// bytes highlighted), not a source of truth anything else depends on, and it's validated against
/// the full painted corpus like every other change in this pass - see
/// `painting_disagreement_fix_log.md`.
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
                // Scoped to Insert/Delete, matching the shape actually observed
                // (`python-refactoring`'s `max_val = max(numbers)`, where `(` is bundled into
                // real content and survives while the lone `)` is dropped): a stray Move/Update
                // punctuation range can legitimately be excluded on its own terms (it's noise from
                // a column shift, not a pairing question), and restoring one regressed several
                // fixtures - `javascript-refactor-arrow-func` painted a Move-only `");"` the
                // human's own ground truth never wanted, once this candidate set included Move.
                && matches!(candidate.operation, TextOperation::Insert | TextOperation::Delete)
        })
        .collect();
    if dropped.is_empty() {
        return result;
    }

    let text = SourceText::new(source);
    let byte_range = |range: &TextRange| {
        (
            text.byte_index(range.start_row, range.start_column),
            text.byte_index(range.end_row, range.end_column),
        )
    };
    let partners = bracket_pair_partners(source);
    let covered = |byte: usize| {
        result.iter().any(|kept| {
            let (s, e) = byte_range(&kept.source);
            s <= byte && byte < e
        })
    };

    let mut restored = Vec::new();
    for candidate in dropped {
        let (start, end) = byte_range(&candidate.source);
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

/// Byte-position -> matching partner byte position, for every `(`/`)`, `[`/`]`, `{`/`}` pair a
/// plain nesting-depth scan of `source` finds - see [`restore_paired_brackets`]'s own doc comment
/// for why this is deliberately not lexer-aware. A mismatched or unbalanced bracket (a genuine
/// syntax error, or - far more likely - one inside a string/comment the scan can't see as such)
/// simply never gets an entry, rather than panicking or guessing.
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

/// [`ranges_for_options`]'s per-range logic for every shape except the interior-line-indentation
/// split (which needs different treatment - see that branch's own comment): the unconditional
/// trailing-whitespace trim, the structural-punctuation filter, then either extending or trimming
/// leading whitespace per [`RenderOptions::leading_whitespace`].
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
            // A range that doesn't read back is left alone rather than silently dropped: this is
            // a display filter, and it has no business deciding that a range it could not
            // interpret is uninteresting.
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

/// Splits a multi-row `Insert`/`Delete` range into one piece per row, each independently trimmed
/// to that row's own real content - [`RenderOptions::leading_whitespace`] off, i.e. the
/// rules doc's indentation choice 2 ("highlight visible characters and whitespace **in the same
/// line** between them", as opposed to choice 3/4's "in all lines"). A row with nothing but
/// whitespace contributes no piece - there is no real character there to select, the same
/// convention `trim_leading_whitespace`/`trim_trailing_whitespace` already hold for a range that is
/// nothing but whitespace start to end.
///
/// Reuses `trim_leading_whitespace`/`trim_trailing_whitespace` per row rather than duplicating
/// their whitespace-walk, by first carving out each row's own slice of `range_match.source` as its
/// own single-row `TextRange`. All pieces share `range_match`'s own `operation` and `destination` -
/// always a placeholder single-point anchor for `Insert`/`Delete` (see
/// `advance_and_build_range`'s doc comment on `last_non_move_range`), never a real per-row
/// cross-file position, so there is no per-row destination to compute and every piece pointing at
/// the same one is exactly as accurate as the single range they replace already was.
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

/// `range` with trailing whitespace removed - its end pulled back to the last non-whitespace
/// character - or `None` if nothing but whitespace remains between `start` and `end`.
///
/// Applied unconditionally by `ranges_for_options`, regardless of `options`: nobody painting a
/// diff by hand ever marks a line's trailing whitespace, and least of all the newline past it -
/// the same rule `human_solver`'s `span_covers` and `columns_on_row`'s callers hold for the
/// rendered highlight and the hand-painted ground truth this is measured against.
///
/// Only the `source` side is narrowed - see [`trim_leading_whitespace`]'s doc comment for why.
pub(crate) fn trim_trailing_whitespace(lines: &[&str], range: &TextRange) -> Option<TextRange> {
    let (start_row, start_column) = (range.start_row, range.start_column);
    let (mut end_row, mut end_column) = (range.end_row, range.end_column);

    loop {
        if (start_row, start_column) >= (end_row, end_column) {
            return None;
        }
        // `end_row` can land at `lines.len()` - one past the last real row - when a range runs
        // through to the very end of a file that has no trailing newline: `advance_and_build_range`
        // reports "through EOF" the same way regardless of whether the file ends in `\n`, but only
        // a file that *does* end in one gets a genuine trailing empty entry from `str::split('\n')`
        // for that row to look up. Without this, `lines.get(end_row)?` returned `None` here and the
        // `?` silently dropped the *entire range* instead of trimming it - confirmed as the root
        // cause of a wholly-appended function rendering with no highlighting at all in
        // `python-api-change` (whose fixture file happens to lack a trailing newline). Treat the
        // missing phantom row exactly like the real one a trailing newline would have provided: step
        // back to the last real row's own end, the same as the loop's own "previous row's newline"
        // branch below already does for the file-ends-in-newline case.
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

/// `range` with leading whitespace removed - its start pushed forward past any whitespace - or
/// `None` if nothing but whitespace remains between `start` and `end`.
///
/// Gated behind [`RenderOptions::leading_whitespace`] in `ranges_for_options`, unlike
/// [`trim_trailing_whitespace`]: leading whitespace before a change (indentation, or a run pushed
/// out by an earlier edit on the same line) is a legitimate thing to show, unlike trailing
/// whitespace or a newline, which no painting - by hand or by this renderer - ever means to mark.
///
/// Only the `source` side is narrowed. The `destination` is a position in the *other* file, whose
/// text this function cannot see, and each side's ranges are filtered independently against their
/// own source - so trimming here and there happens separately and correctly, while `destination`
/// keeps pointing at the untrimmed counterpart region that cross-panel navigation jumps to.
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
            // Past the last character of this row: the newline itself is whitespace, so step over
            // it to the start of the next row.
            None => {
                start_row += 1;
                start_column = 0;
            }
            Some(_) => break,
        }
    }
    Some(TextRange::new(start_row, start_column, end_row, end_column))
}

/// `range` with leading whitespace grown back to column 0 of its start row - the counterpart to
/// [`trim_leading_whitespace`].
///
/// **Why this is needed at all.** A whole inserted or deleted node's range comes straight from
/// tree-sitter's own `node.range()` (`advance_and_build_range`), which never includes the
/// indentation before it - that indentation isn't part of the node, it's the whitespace between
/// siblings. So under [`RenderOptions::FULL`] there was, for this one shape of range, nothing for
/// `leading_whitespace` to keep: [`trim_leading_whitespace`] only ever narrows, and a range that
/// never captured its own indentation has none to narrow away or keep. Confirmed against the
/// corpus's own hand-painted ground truth, which paints indentation as part of the insertion for
/// exactly this shape (a whole new statement/line) - see `rust-hello-world-added-message` and
/// `rust-add-value-to-enum`.
///
/// **Why the caller only calls this when the row's own prefix is pure whitespace, restricts it to
/// `Insert`/`Delete`, and skips any row `ranges_for_options`'s `matched_rows` marks.** Growing a
/// range backward risks claiming bytes that read as part of something else sharing the row - most
/// dangerously indentation in front of content that is itself matched (`Identical`, `Update` or
/// `Move`) and so unchanged, which this function has no way to tell apart from indentation that
/// truly belongs to the inserted/deleted piece just by looking at whitespace: a whole-new-row
/// insert (`rust-hello-world-added-message`) and a same-row insert in front of an otherwise-
/// unchanged, merely reindented declaration (`cpp-add-const-correctness`, where a `const `
/// qualifier is inserted before a `Move`d declaration on the same row) look identical from here -
/// pure whitespace either way. That is why the row-level check has to happen one level up, over
/// the *whole* range list, not inside this function looking at one range's own text.
///
/// Only the `source` side is grown, for the same reason [`trim_leading_whitespace`] only narrows
/// it: `destination` is a position in the other file, whose text this function cannot see.
pub(crate) fn extend_leading_whitespace(lines: &[&str], range: &TextRange) -> TextRange {
    let Some(line) = lines.get(range.start_row) else {
        return range.clone();
    };
    let prefix_end = range.start_column.min(line.len());
    // Always a char boundary, so the slice below can't panic: `range.start_column` is either
    // `node.range()`'s own start (tree-sitter never lands mid-character) or a value
    // `trim_trailing_whitespace` passed through untouched - that function only moves `end`.
    if line[..prefix_end].chars().all(char::is_whitespace) {
        TextRange::new(range.start_row, 0, range.end_row, range.end_column)
    } else {
        range.clone()
    }
}

/// Whether every character `range` covers is structural punctuation or whitespace, given the
/// source already split into rows. `None` if the range falls outside the source.
///
/// Walks the rows rather than materializing the covered text: a multi-row range's text would have
/// to be joined into a fresh `String` just to be scanned once and dropped, and the covered rows
/// are exactly what needs checking either way.
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
        // The newline joining this row to the next is itself whitespace, so a multi-row range that
        // is blank on every row stays structural-only.
        if row < range.end_row {
            saw_any = true;
        }
    }
    // An empty range covers nothing, and `is_structural_only` deliberately says a zero-width
    // placeholder is not structural - it marks a position the other side changed.
    Some(saw_any)
}
