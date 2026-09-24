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

use crate::test;
use anyhow::Result;

use super::plain_text_diff::*;
use super::render_options::*;
use super::summary::*;
use super::*;

fn text_range_on(row: usize, start: usize, end: usize) -> TextRange {
    TextRange::new(row, start, row, end)
}

fn changed(row: usize, start: usize, end: usize) -> RangeMatch {
    RangeMatch {
        source: text_range_on(row, start, end),
        destination: text_range_on(row, start, end),
        operation: TextOperation::Update,
    }
}

/// The corpus was painted narrow under both presets, so `whole_pair_updates` is off in both.
#[test]
fn neither_preset_turns_on_whole_pair_updates() {
    const { assert!(!RenderOptions::MINIMAL.whole_pair_updates) };
    const { assert!(!RenderOptions::FULL.whole_pair_updates) };
}

/// `rust-next-font-imports-generator`'s `Minimal` and `Full` paintings disagree on this.
#[test]
fn minimal_and_full_disagree_on_paint_reindent_only_moves() {
    const { assert!(!RenderOptions::MINIMAL.paint_reindent_only_moves) };
    const { assert!(RenderOptions::FULL.paint_reindent_only_moves) };
}

/// A `NestedConditionCollapse`-tagged body paints `Move` only under `paint_reindent_only_moves`.
#[test]
fn paint_reindent_only_moves_gates_only_tagged_nodes() {
    let body = "\x20               step_one();\n\
                 \x20               step_two();\n\
                 \x20               step_three();\n\
                 \x20               step_four();\n\
                 \x20               step_five();\n\
                 \x20               step_six();\n";
    let before = Code::from_string(
        &format!(
            "fn f() {{\n\
             \x20   if let A(a) = x {{\n\
             \x20       if let B(b) = y {{\n\
             {body}\
             \x20       }}\n\
             \x20   }}\n\
             }}\n"
        ),
        &crate::code::Language::Rust,
    );
    let after = Code::from_string(
        &format!(
            "fn f() {{\n\
             \x20   if let A(a) = x\n\
             \x20       && let B(b) = y\n\
             \x20   {{\n\
             {body}\
             \x20   }}\n\
             }}\n"
        ),
        &crate::code::Language::Rust,
    );
    let ast = crate::diff::diff_code(&before, &after);
    let node_cache = crate::diff::NodeCache::build(&before, &after);

    let painted = TextDiff::from_with_options(
        &before,
        &after,
        ast.ast.as_ref().unwrap(),
        &node_cache,
        RenderOptions::FULL,
    );
    assert!(
        painted
            .all(0)
            .iter()
            .any(|r| r.operation == TextOperation::Move
                && r.source.start_row >= 2
                && r.source.start_row <= 8),
        "paint_reindent_only_moves: true must still paint the reindented body Move"
    );

    let unpainted = TextDiff::from_with_options(
        &before,
        &after,
        ast.ast.as_ref().unwrap(),
        &node_cache,
        RenderOptions::MINIMAL,
    );
    assert!(
        !unpainted
            .all(0)
            .iter()
            .any(|r| r.operation == TextOperation::Move
                && r.source.start_row >= 2
                && r.source.start_row <= 8),
        "paint_reindent_only_moves: false must leave the tagged reindented body unpainted"
    );
}

/// `MINIMAL` suppresses a displacement-only `Move`; `FULL` keeps it.
#[test]
fn minimal_and_full_disagree_on_paint_displaced_moves() {
    const { assert!(!RenderOptions::MINIMAL.paint_displaced_moves) };
    const { assert!(RenderOptions::FULL.paint_displaced_moves) };
}

/// `shellscript-ansible-ansible-a-small-add`'s shape: a single-row node keeps its text and its
/// place beside the one edit on its row, but a line inserted above changes its row index. The
/// `FULL` half pins that this stays a preset split, not a blanket suppression.
#[test]
fn paint_displaced_moves_gates_a_node_pushed_down_by_an_insertion_above_it() {
    let before = Code::from_string(
        "fn main() {\n         \x20   let total = compute(first, second);\n         }\n",
        &crate::code::Language::Rust,
    );
    let after = Code::from_string(
        "fn main() {\n         \x20   setup();\n         \x20   let total = compute(extra, first, second);\n         }\n",
        &crate::code::Language::Rust,
    );
    let ast = crate::diff::diff_code(&before, &after);
    let node_cache = crate::diff::NodeCache::build(&before, &after);

    // The call row's unchanged tail, pushed down by `setup();` and right by `extra, `. Asserted as
    // "is there an `Identical` range there" because under `FULL` it is swallowed into a larger
    // multi-row `Move`, not a range of its own.
    let identical_tail_on_the_call_row = |options| {
        TextDiff::from_with_options(
            &before,
            &after,
            ast.ast.as_ref().unwrap(),
            &node_cache,
            options,
        )
        .all(0)
        .iter()
        .any(|r| {
            r.operation == TextOperation::Identical
                && r.source.start_row == 1
                && r.source.start_column == 33
        })
    };

    assert!(
        !identical_tail_on_the_call_row(RenderOptions::FULL),
        "paint_displaced_moves: true must keep painting the displaced tail Move"
    );
    assert!(
        identical_tail_on_the_call_row(RenderOptions::MINIMAL),
        "paint_displaced_moves: false must leave the purely displaced tail unpainted"
    );
}

/// The multi-row half (`rust-adding-a-variable-and-test-with-comments`): a closure whose first row
/// carries the one edit, pushed down by a line inserted above, other rows unchanged. `FULL` still
/// paints it.
#[test]
fn paint_displaced_moves_gates_a_multi_row_node_edited_on_its_first_row() {
    let before = Code::from_string(
        "fn main() {\n    let x = values.filter(row_len).map(|value| {\n        println!(\"{value}\");\n    });\n}\n",
        &crate::code::Language::Rust,
    );
    let after = Code::from_string(
        "fn main() {\n    setup();\n    let x = values.filter(paint_row_len).map(|value| {\n        println!(\"{value}\");\n    });\n}\n",
        &crate::code::Language::Rust,
    );
    let ast = crate::diff::diff_code(&before, &after);
    let node_cache = crate::diff::NodeCache::build(&before, &after);

    let multi_row_move_on_the_closure_row = |options| {
        TextDiff::from_with_options(
            &before,
            &after,
            ast.ast.as_ref().unwrap(),
            &node_cache,
            options,
        )
        .all(0)
        .iter()
        .any(|r| {
            r.operation == TextOperation::Move
                && r.source.start_row == 1
                && r.source.end_row > r.source.start_row
        })
    };

    assert!(
        multi_row_move_on_the_closure_row(RenderOptions::FULL),
        "paint_displaced_moves: true must keep painting the displaced closure Move"
    );
    assert!(
        !multi_row_move_on_the_closure_row(RenderOptions::MINIMAL),
        "paint_displaced_moves: false must leave the displaced closure unpainted"
    );
}

/// Two walks naming one relocation over extents a column apart agree; neither claim is withdrawn
/// under `FULL`, while `MINIMAL` keeps only the more economical account.
#[test]
fn reconcile_moves_keeps_two_overlapping_accounts_of_one_relocation() {
    let range_match = |source: TextRange, destination: TextRange, operation| RangeMatch {
        source,
        destination,
        operation,
    };
    // The before walk calls rows 3..9 moved; the after walk names the same relocation from one
    // column further in, which no exact-extent lookup can match.
    let mut before = [range_match(
        TextRange::new(3, 4, 9, 0),
        TextRange::new(3, 3, 9, 0),
        TextOperation::Move,
    )];
    let mut after = [range_match(
        TextRange::new(3, 2, 9, 0),
        TextRange::new(3, 5, 9, 0),
        TextOperation::Move,
    )];

    let mut minimal = (before.clone(), after.clone());
    reconcile_moves(&mut minimal.0, &mut minimal.1, RenderOptions::MINIMAL);
    reconcile_moves(&mut before, &mut after, RenderOptions::FULL);

    assert_eq!(
        (before[0].operation.clone(), after[0].operation.clone()),
        (TextOperation::Move, TextOperation::Move),
        "an overlapping account of the same relocation is agreement, not a disagreement to \
         resolve by withdrawing one side"
    );
    assert_eq!(
        (
            minimal.0[0].operation.clone(),
            minimal.1[0].operation.clone()
        ),
        (TextOperation::Move, TextOperation::Identical),
        "MINIMAL keeps the more economical single account - mirroring the relocation onto the \
         second panel doubles the bytes it paints"
    );
}

fn one_row(row: usize) -> TextRange {
    TextRange::new(row, 0, row + 1, 0)
}

fn row_pair(source_row: usize, destination_row: usize, operation: TextOperation) -> RangeMatch {
    RangeMatch {
        source: one_row(source_row),
        destination: one_row(destination_row),
        operation,
    }
}

/// One line moved below five: the before walk blames the five, the after walk the one. The
/// smaller account wins on both sides, and the winner's counterpart is promoted to `Move`.
#[test]
fn reconcile_moves_believes_the_side_that_blames_fewer_rows() {
    let mut before = vec![row_pair(0, 5, TextOperation::Identical)];
    before.extend((1..6).map(|row| row_pair(row, row - 1, TextOperation::Move)));
    let mut after = (0..5)
        .map(|row| row_pair(row, row + 1, TextOperation::Identical))
        .collect::<Vec<_>>();
    after.push(row_pair(5, 0, TextOperation::Move));

    reconcile_moves(&mut before, &mut after, RenderOptions::FULL);

    let moved_rows = |ranges: &[RangeMatch]| {
        ranges
            .iter()
            .filter(|r| r.operation == TextOperation::Move)
            .map(|r| r.source.start_row)
            .collect::<Vec<_>>()
    };
    assert_eq!(moved_rows(&before), vec![0]);
    assert_eq!(moved_rows(&after), vec![5]);
}

/// Two accounts of equal size: the before side's wins, so the result is a function of the input.
#[test]
fn reconcile_moves_breaks_a_tie_in_favour_of_the_before_side() {
    let mut before = vec![
        row_pair(0, 1, TextOperation::Move),
        row_pair(1, 0, TextOperation::Identical),
    ];
    let mut after = vec![
        row_pair(0, 1, TextOperation::Move),
        row_pair(1, 0, TextOperation::Identical),
    ];

    reconcile_moves(&mut before, &mut after, RenderOptions::FULL);

    let operations = |ranges: &[RangeMatch]| {
        ranges
            .iter()
            .map(|r| r.operation.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        operations(&before),
        vec![TextOperation::Move, TextOperation::Identical]
    );
    assert_eq!(
        operations(&after),
        vec![TextOperation::Identical, TextOperation::Move]
    );
}

/// `paint_resized_moves` is the third axis the two presets disagree on.
#[test]
fn minimal_and_full_disagree_on_paint_resized_moves() {
    const { assert!(!RenderOptions::MINIMAL.paint_resized_moves) };
    const { assert!(RenderOptions::FULL.paint_resized_moves) };
}

/// The first two options are post-filters and never need a rebuild; every other one does.
#[test]
fn only_the_post_filters_skip_a_rebuild() {
    let base = RenderOptions::FULL;
    for index in 0..base.options().len() {
        let mut toggled = base;
        toggled.toggle(index);
        assert_eq!(
            toggled.needs_rebuild_from(&base),
            index >= 2,
            "option {index} ({})",
            base.options()[index].0
        );
    }
    assert!(!base.needs_rebuild_from(&base));
}

/// `whole_identifier_updates` is the fourth, and ground-truth invariant 16 is why.
#[test]
fn minimal_and_full_disagree_on_whole_identifier_updates() {
    const { assert!(!RenderOptions::MINIMAL.whole_identifier_updates) };
    const { assert!(RenderOptions::FULL.whole_identifier_updates) };
}

/// The lone brackets and separators the painted corpus's `Minimal` drops.
#[test]
fn minimal_drops_the_punctuation_the_painted_corpus_drops() {
    for text in ["(", ")", "):", ";", "{", "}", "[]", "  ", " ,\n", "( )"] {
        assert!(
            is_structural_only(text),
            "{text:?} should be structural-only"
        );
    }
}

/// Operators look like punctuation and are the entire content of the change they appear in.
/// Dropping them would report a different diff, not a tighter one.
#[test]
fn minimal_keeps_operators_and_anything_carrying_meaning() {
    for text in ["+", "=", "<=", "&&", "=>", "foo", "1", "_", "a,", "->"] {
        assert!(!is_structural_only(text), "{text:?} should survive Minimal");
    }
}

/// A zero-width range marks where the other side gained or lost text - the only mark that side
/// has. Treating "no characters" as "only structural characters" would erase it.
#[test]
fn an_empty_range_is_not_structural() {
    assert!(!is_structural_only(""));
}

#[test]
fn full_returns_every_range_unchanged() {
    let source = "foo(bar);\n";
    let ranges = vec![changed(0, 3, 4), changed(0, 4, 7)];

    assert_eq!(
        ranges_for_options(&ranges, source, RenderOptions::FULL),
        ranges,
        "Full is exactly TextDiff::all"
    );
}

#[test]
fn minimal_drops_a_standalone_bracket_but_keeps_its_neighbour() {
    let source = "foo(bar);\n";
    // `(` alone, then `bar`.
    let ranges = vec![changed(0, 3, 4), changed(0, 4, 7)];

    let minimal = ranges_for_options(&ranges, source, RenderOptions::MINIMAL);

    assert_eq!(minimal.len(), 1, "got {minimal:?}");
    assert_eq!(minimal[0].source, text_range_on(0, 4, 7));
}

/// A bracket *inside* a larger range is not a standalone bracket - only whole ranges are
/// dropped, never parts of one, so a surviving range is byte-for-byte what `Full` shows.
#[test]
fn minimal_keeps_a_range_that_merely_contains_punctuation() {
    let source = "foo(bar);\n";
    let ranges = vec![changed(0, 0, 9)];

    assert_eq!(
        ranges_for_options(&ranges, source, RenderOptions::MINIMAL),
        ranges
    );
}

/// `Identical` ranges are the unpainted background, so dropping them changes nothing on
/// screen - but `line_operations` reads them to colour rows, and the two modes stay
/// comparable only if their range lists differ by content ranges alone.
#[test]
fn minimal_keeps_identical_ranges_even_when_they_are_pure_punctuation() {
    let source = "foo(bar);\n";
    let identical = RangeMatch {
        source: text_range_on(0, 3, 4),
        destination: text_range_on(0, 3, 4),
        operation: TextOperation::Identical,
    };

    assert_eq!(
        ranges_for_options(
            std::slice::from_ref(&identical),
            source,
            RenderOptions::MINIMAL
        ),
        vec![identical]
    );
}

/// Painting by hand, nobody marks the indentation in front of a change or the blank running
/// off the end of the line - a highlight that includes them reads as though the blank space
/// were part of the edit.
#[test]
fn minimal_trims_leading_and_trailing_whitespace_off_a_range() {
    let source = "    foo   \n";
    let ranges = vec![changed(0, 0, 10)];

    let minimal = ranges_for_options(&ranges, source, RenderOptions::MINIMAL);

    assert_eq!(minimal.len(), 1);
    assert_eq!(
        minimal[0].source,
        text_range_on(0, 4, 7),
        "the range should cover exactly `foo`"
    );
}

/// Interior whitespace sits *between* two things the range is genuinely about; cutting there
/// would report one edit as two.
#[test]
fn minimal_keeps_whitespace_inside_a_range() {
    let source = "a   b\n";
    let ranges = vec![changed(0, 0, 5)];

    assert_eq!(
        ranges_for_options(&ranges, source, RenderOptions::MINIMAL)[0].source,
        text_range_on(0, 0, 5)
    );
}

/// Trailing whitespace is not an option: even `FULL` trims it.
#[test]
fn full_keeps_leading_but_still_trims_trailing_whitespace() {
    let source = "    foo   \n";
    let ranges = vec![changed(0, 0, 10)];

    let full = ranges_for_options(&ranges, source, RenderOptions::FULL);

    assert_eq!(full.len(), 1);
    assert_eq!(
        full[0].source,
        text_range_on(0, 0, 7),
        "leading spaces survive, trailing ones do not"
    );
}

/// A multi-line block inserted on its own: with `leading_whitespace` off each row is highlighted
/// without its indentation, including interior rows that `FULL` keeps whole.
#[test]
fn leading_whitespace_off_splits_a_multiline_insert_per_row_trimmed() {
    let source = "    def added_function():\n        print(\"added\")\n";
    let range = RangeMatch {
        source: TextRange::new(0, 4, 1, 22),
        destination: TextRange::zero(),
        operation: TextOperation::Insert,
    };
    let options = RenderOptions {
        paint_reindent_only_moves: true,
        ..RenderOptions::MINIMAL
    };

    let result = ranges_for_options(std::slice::from_ref(&range), source, options);

    assert_eq!(
        result,
        vec![
            RangeMatch {
                source: TextRange::new(0, 4, 0, 25),
                destination: TextRange::zero(),
                operation: TextOperation::Insert,
            },
            RangeMatch {
                source: TextRange::new(1, 8, 1, 22),
                destination: TextRange::zero(),
                operation: TextOperation::Insert,
            },
        ],
        "row 0 keeps its own real content ('def added_function():'), row 1's own indentation \
         is trimmed independently rather than kept whole"
    );
}

/// A blank row in the middle of a multi-line insert has no real content of its own to select,
/// so it contributes no piece at all - not an empty one.
#[test]
fn leading_whitespace_off_drops_a_blank_interior_row() {
    let source = "line one\n\nline three\n";
    let range = RangeMatch {
        source: TextRange::new(0, 0, 2, 10),
        destination: TextRange::zero(),
        operation: TextOperation::Insert,
    };
    let options = RenderOptions {
        paint_reindent_only_moves: true,
        ..RenderOptions::MINIMAL
    };

    let result = ranges_for_options(std::slice::from_ref(&range), source, options);

    assert_eq!(
        result
            .iter()
            .map(|r| r.source.start_row)
            .collect::<Vec<_>>(),
        vec![0, 2],
        "the blank row 1 must contribute nothing: {result:?}"
    );
}

/// The per-row split applies only to `Insert`/`Delete`: a multi-row `Update` stays one range.
#[test]
fn leading_whitespace_off_does_not_split_a_multiline_update() {
    let source = "one\ntwo\nthree\n";
    let range = RangeMatch {
        source: TextRange::new(0, 0, 2, 5),
        destination: TextRange::new(0, 0, 2, 5),
        operation: TextOperation::Update,
    };
    let options = RenderOptions {
        paint_reindent_only_moves: true,
        ..RenderOptions::MINIMAL
    };

    let result = ranges_for_options(std::slice::from_ref(&range), source, options);

    assert_eq!(
        result.len(),
        1,
        "an Update range must not be split: {result:?}"
    );
}

/// The structural-punctuation filter still applies per row after the split - a row that is
/// nothing but a standalone bracket is dropped under `MINIMAL`'s `structural_punctuation: false`
/// exactly as a single-row range in the same shape already is.
#[test]
fn leading_whitespace_off_still_drops_a_structural_only_row() {
    let source = "{\n    body();\n}\n";
    let range = RangeMatch {
        source: TextRange::new(0, 0, 2, 1),
        destination: TextRange::zero(),
        operation: TextOperation::Insert,
    };
    let options = RenderOptions {
        paint_reindent_only_moves: true,
        ..RenderOptions::MINIMAL
    };

    let result = ranges_for_options(std::slice::from_ref(&range), source, options);

    assert_eq!(
        result
            .iter()
            .map(|r| r.source.start_row)
            .collect::<Vec<_>>(),
        vec![1],
        "rows 0 ('{{') and 2 ('}}') are standalone punctuation and must be dropped: {result:?}"
    );
}

/// `MINIMAL`'s standalone-punctuation behavior is independent of leading-whitespace: a range
/// that survives (real content, not pure punctuation) still gets its leading whitespace kept
/// when only `structural_punctuation` is off.
#[test]
fn structural_punctuation_off_alone_still_keeps_leading_whitespace() {
    let source = "    foo   \n";
    let ranges = vec![changed(0, 0, 10)];

    let options = RenderOptions {
        leading_whitespace: true,
        structural_punctuation: false,
        whole_pair_updates: false,
        paint_reindent_only_moves: true,
        paint_displaced_moves: true,
        paint_resized_moves: true,
        whole_identifier_updates: true,
    };
    let result = ranges_for_options(&ranges, source, options);

    assert_eq!(result.len(), 1);
    assert_eq!(result[0].source, text_range_on(0, 0, 7));
}

/// Symmetric case: `leading_whitespace` off with `structural_punctuation` on trims leading
/// whitespace but does not drop a range merely because part of it is punctuation - only a
/// range that is *nothing but* punctuation is ever dropped, and that's gated by the other
/// option entirely.
#[test]
fn leading_whitespace_off_alone_still_keeps_a_range_containing_punctuation() {
    let source = "    foo();   \n";
    let ranges = vec![changed(0, 0, 10)]; // "    foo();" - leading spaces, then `foo();`

    let options = RenderOptions {
        leading_whitespace: false,
        structural_punctuation: true,
        whole_pair_updates: false,
        paint_reindent_only_moves: true,
        paint_displaced_moves: true,
        paint_resized_moves: true,
        whole_identifier_updates: true,
    };
    let result = ranges_for_options(&ranges, source, options);

    assert_eq!(result.len(), 1);
    assert_eq!(
        result[0].source,
        text_range_on(0, 4, 10),
        "trims the leading spaces, keeps `foo();` whole - it isn't pure punctuation"
    );
}

/// `python-refactoring`, minimized: merging bundles `max_val = max(` into one `Insert` and leaves
/// `)` alone. The lone `)` is restored so a reader never sees `(` without it.
#[test]
fn a_lone_closing_paren_is_restored_when_its_open_partner_survives() {
    let source = "max_val = max(numbers)";
    let ranges = vec![
        RangeMatch {
            source: text_range_on(0, 0, 14), // "max_val = max("
            destination: TextRange::zero(),
            operation: TextOperation::Insert,
        },
        RangeMatch {
            source: text_range_on(0, 14, 21), // "numbers" - matched elsewhere, Identical
            destination: text_range_on(0, 14, 21),
            operation: TextOperation::Identical,
        },
        RangeMatch {
            source: text_range_on(0, 21, 22), // ")" - alone, purely structural
            destination: TextRange::zero(),
            operation: TextOperation::Insert,
        },
    ];

    let result = ranges_for_options(&ranges, source, RenderOptions::MINIMAL);

    assert!(
        result.iter().any(|r| r.source == text_range_on(0, 21, 22)),
        "the lone ')' must be restored: {result:?}"
    );
}

/// A pair whose halves are both standalone stays dropped.
#[test]
fn a_pair_that_is_entirely_standalone_punctuation_stays_dropped() {
    let source = "(x)";
    let ranges = vec![
        RangeMatch {
            source: text_range_on(0, 0, 1), // "(" alone
            destination: TextRange::zero(),
            operation: TextOperation::Insert,
        },
        RangeMatch {
            source: text_range_on(0, 1, 2), // "x" - matched elsewhere
            destination: text_range_on(0, 1, 2),
            operation: TextOperation::Identical,
        },
        RangeMatch {
            source: text_range_on(0, 2, 3), // ")" alone
            destination: TextRange::zero(),
            operation: TextOperation::Insert,
        },
    ];

    let result = ranges_for_options(&ranges, source, RenderOptions::MINIMAL);

    assert!(
        !result.iter().any(|r| r.source == text_range_on(0, 0, 1)),
        "neither half of a wholly standalone pair should be restored: {result:?}"
    );
    assert!(!result.iter().any(|r| r.source == text_range_on(0, 2, 3)));
}

/// Restoration is scoped to `Insert`/`Delete`: `javascript-refactor-arrow-func` paints a lone `");"`
/// `Move` that its ground truth never shows.
#[test]
fn a_lone_move_bracket_is_not_restored() {
    let source = "max_val = max(numbers)";
    let ranges = vec![
        RangeMatch {
            source: text_range_on(0, 0, 14),
            destination: TextRange::zero(),
            operation: TextOperation::Insert,
        },
        RangeMatch {
            source: text_range_on(0, 14, 21),
            destination: text_range_on(0, 14, 21),
            operation: TextOperation::Identical,
        },
        RangeMatch {
            source: text_range_on(0, 21, 22), // ")" alone, but Move this time
            destination: text_range_on(0, 21, 22),
            operation: TextOperation::Move,
        },
    ];

    let result = ranges_for_options(&ranges, source, RenderOptions::MINIMAL);

    assert!(
        !result.iter().any(|r| r.source == text_range_on(0, 21, 22)),
        "a lone Move bracket must not be restored: {result:?}"
    );
}

/// Trimming narrows the source only. The destination is a position in the *other* file, whose
/// text this side cannot see, and cross-panel navigation jumps to it.
#[test]
fn trimming_leaves_the_destination_alone() {
    let source = "  foo  \n";
    let ranges = vec![RangeMatch {
        source: text_range_on(0, 0, 7),
        destination: text_range_on(3, 0, 7),
        operation: TextOperation::Update,
    }];

    let minimal = ranges_for_options(&ranges, source, RenderOptions::MINIMAL);

    assert_eq!(minimal[0].source, text_range_on(0, 2, 5));
    assert_eq!(minimal[0].destination, text_range_on(3, 0, 7));
}

/// A multi-row range must not be judged by its first row alone: whitespace there says nothing
/// about the rows below it.
#[test]
fn a_multi_row_range_with_content_below_a_blank_first_row_survives() {
    let source = "   \n  keep\n";
    let ranges = vec![RangeMatch {
        source: TextRange::new(0, 0, 1, 6),
        destination: TextRange::new(0, 0, 1, 6),
        operation: TextOperation::Update,
    }];

    let minimal = ranges_for_options(&ranges, source, RenderOptions::MINIMAL);

    assert_eq!(minimal.len(), 1, "got {minimal:?}");
    assert_eq!(
        minimal[0].source,
        TextRange::new(1, 2, 1, 6),
        "and it should trim down to `keep`"
    );
}

/// A range that is nothing but blank rows is still dropped.
#[test]
fn a_multi_row_range_of_only_whitespace_is_dropped() {
    let source = "   \n   \n";
    let ranges = vec![RangeMatch {
        source: TextRange::new(0, 0, 1, 3),
        destination: TextRange::new(0, 0, 1, 3),
        operation: TextOperation::Update,
    }];

    assert!(ranges_for_options(&ranges, source, RenderOptions::MINIMAL).is_empty());
}

/// A range that doesn't read back is left alone. This is a display filter; deciding that
/// something it could not interpret is uninteresting is not its call to make.
#[test]
fn minimal_keeps_a_range_it_cannot_read() {
    let source = "short\n";
    let ranges = vec![changed(99, 0, 4)];

    assert_eq!(
        ranges_for_options(&ranges, source, RenderOptions::MINIMAL),
        ranges
    );
}

/// Every non-Identical range's source row span, in order - the shape of assertion these tests
/// care about (what changed, and in what order), without pinning down destination anchors
/// line by line.
fn changed_row_spans(ranges: &[RangeMatch]) -> Vec<(TextOperation, usize, usize)> {
    ranges
        .iter()
        .filter(|r| r.operation != TextOperation::Identical)
        .map(|r| (r.operation.clone(), r.source.start_row, r.source.end_row))
        .collect()
}

#[test]
fn plain_text_line_diff_matches_identical_lines() {
    let (before, after) = plain_text_line_diff("a\nb\nc\n", "a\nb\nc\n");
    assert!(changed_row_spans(&before).is_empty());
    assert!(changed_row_spans(&after).is_empty());
    assert_eq!(before.len(), 3, "every line should get an Identical range");
    assert!(
        before
            .iter()
            .all(|r| r.operation == TextOperation::Identical)
    );
}

#[test]
fn plain_text_line_diff_finds_a_pure_insertion() {
    let (before, after) = plain_text_line_diff("a\nc\n", "a\nb\nc\n");
    assert_eq!(changed_row_spans(&before), vec![]);
    assert_eq!(
        changed_row_spans(&after),
        vec![(TextOperation::Insert, 1, 2)]
    );
}

#[test]
fn plain_text_line_diff_finds_a_pure_deletion() {
    let (before, after) = plain_text_line_diff("a\nb\nc\n", "a\nc\n");
    assert_eq!(
        changed_row_spans(&before),
        vec![(TextOperation::Delete, 1, 2)]
    );
    assert_eq!(changed_row_spans(&after), vec![]);
}

/// Two lines sharing no affix are not a rewrite of each other: delete plus insert, as `diff -u`.
#[test]
fn plain_text_line_diff_treats_a_dissimilar_changed_line_as_delete_plus_insert() {
    let (before, after) = plain_text_line_diff("a\nOLD\nc\n", "a\nNEW\nc\n");
    assert_eq!(
        changed_row_spans(&before),
        vec![(TextOperation::Delete, 1, 2)]
    );
    assert_eq!(
        changed_row_spans(&after),
        vec![(TextOperation::Insert, 1, 2)]
    );
}

/// Every range on one row, as `(operation, start_column, end_column)` in byte columns.
fn row_column_spans(ranges: &[RangeMatch], row: usize) -> Vec<(TextOperation, usize, usize)> {
    ranges
        .iter()
        .filter(|r| r.source.start_row == row && r.source.end_row == row)
        .map(|r| {
            (
                r.operation.clone(),
                r.source.start_column,
                r.source.end_column,
            )
        })
        .collect()
}

/// A wide line whose one field changed highlights that field, not the row (a regenerated CSV row).
#[test]
fn plain_text_line_diff_narrows_a_changed_line_to_the_changed_part() {
    let before = "h\nname,alpha,beta,gamma,delta,37.282,epsilon,zeta\n";
    let after = "h\nname,alpha,beta,gamma,delta,37.860,epsilon,zeta\n";
    let (before_ranges, after_ranges) = plain_text_line_diff(before, after);

    // "37." is common prefix, "epsilon,zeta" common suffix; only "282"/"860" differ.
    let prefix_end = "name,alpha,beta,gamma,delta,37.".len();
    let middle_end = prefix_end + "282".len();
    assert_eq!(
        row_column_spans(&before_ranges, 1),
        vec![
            (TextOperation::Identical, 0, prefix_end),
            (TextOperation::Update, prefix_end, middle_end),
            (
                TextOperation::Identical,
                middle_end,
                "name,alpha,beta,gamma,delta,37.282,epsilon,zeta".len()
            ),
        ]
    );
    assert_eq!(
        row_column_spans(&after_ranges, 1).len(),
        3,
        "both sides must produce the same number of sub-ranges - see intra_line_ranges"
    );
}

/// A pure block insert stays one range, not one per line, so `n`/`p` navigation steps over it once.
#[test]
fn plain_text_line_diff_keeps_a_block_insert_merged() {
    let before = "same\n";
    let after = "same\nwholly different one\nwholly different two\nwholly different three\n";
    let (_, after_ranges) = plain_text_line_diff(before, after);
    assert_eq!(
        changed_row_spans(&after_ranges),
        vec![(TextOperation::Insert, 1, 4)],
        "three unrelated inserted lines must stay one merged range"
    );
}

/// Rows inserted among changed rows knock the sides out of step; `plan_gap` resynchronises
/// instead of ending refinement for the rest of the run.
#[test]
fn plain_text_line_diff_resynchronises_after_an_inserted_line() {
    let before = "row-a,1\nrow-b,1\nrow-c,1\n";
    let after = "row-a,2\nBRAND NEW UNRELATED LINE\nrow-b,2\nrow-c,2\n";
    let (before_ranges, after_ranges) = plain_text_line_diff(before, after);

    for (before_row, after_row) in [(0usize, 0usize), (1, 2), (2, 3)] {
        assert_eq!(
            row_column_spans(&before_ranges, before_row),
            vec![
                (TextOperation::Identical, 0, "row-a,".len()),
                (TextOperation::Update, "row-a,".len(), "row-a,1".len()),
            ],
            "before row {before_row} should narrow to its trailing digit"
        );
        assert_eq!(
            row_column_spans(&after_ranges, after_row).len(),
            2,
            "after row {after_row} must stay symmetric with its partner"
        );
    }
    assert!(
        changed_row_spans(&after_ranges).contains(&(TextOperation::Insert, 1, 2)),
        "the genuinely new line must still read as an insert: {:?}",
        changed_row_spans(&after_ranges)
    );
}

/// Byte columns, not characters: a multi-byte character before the change shifts every later
/// offset, and getting it wrong slices mid-character downstream.
#[test]
fn plain_text_line_diff_intra_line_columns_are_byte_offsets() {
    let before = "x\nprefix — value 1 suffix\n";
    let after = "x\nprefix — value 2 suffix\n";
    let (before_ranges, _) = plain_text_line_diff(before, after);

    let prefix_end = "prefix — value ".len(); // 17 bytes, 15 chars - the em dash is 3 bytes
    assert_eq!(
        row_column_spans(&before_ranges, 1),
        vec![
            (TextOperation::Identical, 0, prefix_end),
            (TextOperation::Update, prefix_end, prefix_end + 1),
            (
                TextOperation::Identical,
                prefix_end + 1,
                "prefix — value 1 suffix".len()
            ),
        ]
    );
    assert_eq!(
        prefix_end, 17,
        "sanity: the em dash must count as 3 bytes, not 1 char"
    );
}

/// Matches separated by gaps on both sides: each side's gaps are walked in its own row space, not
/// zipped together.
#[test]
fn plain_text_line_diff_handles_non_contiguous_matches() {
    let before = "same0\nDEL_A\nsame1\nDEL_B\nDEL_C\nsame2\n";
    let after = "same0\nINS_A\nINS_B\nsame1\nsame2\n";
    let (before_ranges, after_ranges) = plain_text_line_diff(before, after);

    assert_eq!(
        changed_row_spans(&before_ranges),
        vec![(TextOperation::Delete, 1, 2), (TextOperation::Delete, 3, 5)],
        "before's two unmatched runs (rows 1 and 3-4) must stay separate, not merge across \
         the row-2 match"
    );
    assert_eq!(
        changed_row_spans(&after_ranges),
        vec![(TextOperation::Insert, 1, 3)],
        "after's contiguous unmatched run (rows 1-2) must merge into one range"
    );

    // same0 (before row 0) matches after row 0; same1 (before row 2) matches after row 3;
    // same2 (before row 5) matches after row 4.
    let matches: Vec<_> = before_ranges
        .iter()
        .filter(|r| r.operation == TextOperation::Identical)
        .map(|r| (r.source.start_row, r.destination.start_row))
        .collect();
    assert_eq!(matches, vec![(0, 0), (2, 3), (5, 4)]);
}

/// An unmatched run's anchor is just past the preceding match's destination, not its own row
/// number, which is in the other side's coordinate space.
#[test]
fn plain_text_line_diff_anchors_unmatched_runs_at_the_preceding_matchs_destination() {
    // before: same0, DEL, same1        (3 lines)
    // after:  same0, INS_A, INS_B, same1  (4 lines) - "same1" sits at a different row on
    // each side (before row 2, after row 3), so a correct anchor must use the *destination*
    // coordinate space, not reuse the source row number.
    let before = "same0\nDEL\nsame1\n";
    let after = "same0\nINS_A\nINS_B\nsame1\n";
    let (before_ranges, after_ranges) = plain_text_line_diff(before, after);

    let delete = before_ranges
        .iter()
        .find(|r| r.operation == TextOperation::Delete)
        .expect("before should have one Delete range");
    assert_eq!(
        delete.destination.start_row, 1,
        "the deleted before-row-1 line has no real counterpart, so its cross-highlight \
         anchor should sit right after same0's match (after row 0), i.e. after row 1"
    );

    let insert = after_ranges
        .iter()
        .find(|r| r.operation == TextOperation::Insert)
        .expect("after should have one Insert range");
    assert_eq!(
        insert.destination.start_row, 1,
        "the inserted after-rows have no real counterpart, so its cross-highlight anchor \
         should sit right after same0's match (before row 0), i.e. before row 1"
    );
}

#[test]
fn plain_text_line_diff_handles_empty_before_as_a_pure_insertion() {
    let (before, after) = plain_text_line_diff("", "a\nb\n");
    assert!(before.is_empty());
    assert_eq!(
        changed_row_spans(&after),
        vec![(TextOperation::Insert, 0, 2)]
    );
}

#[test]
fn plain_text_line_diff_handles_empty_after_as_a_pure_deletion() {
    let (before, after) = plain_text_line_diff("a\nb\n", "");
    assert_eq!(
        changed_row_spans(&before),
        vec![(TextOperation::Delete, 0, 2)]
    );
    assert!(after.is_empty());
}

#[test]
fn plain_text_line_diff_treats_two_empty_files_as_no_changes() {
    let (before, after) = plain_text_line_diff("", "");
    assert!(before.is_empty());
    assert!(after.is_empty());
}

/// Past the edit-distance cap, the whole file is one `Delete` and one `Insert`. Uses a small cap:
/// reaching the real `PLAIN_TEXT_MAX_EDIT` costs quadratic memory.
#[test]
fn plain_text_line_diff_replaces_the_whole_file_past_the_edit_cap() {
    const SMALL_CAP: usize = 20;
    let before: String = (0..SMALL_CAP + 10)
        .map(|i| format!("before-unique-line-{i}\n"))
        .collect();
    let after: String = (0..SMALL_CAP + 10)
        .map(|i| format!("after-unique-line-{i}\n"))
        .collect();
    let (before_ranges, after_ranges) =
        plain_text_line_diff_with_max_edit(&before, &after, SMALL_CAP);

    assert_eq!(before_ranges.len(), 1);
    assert_eq!(before_ranges[0].operation, TextOperation::Delete);
    assert_eq!(
        before_ranges[0].source.end_row,
        SMALL_CAP + 10,
        "the single Delete range should cover every line, not just part of the file"
    );

    assert_eq!(after_ranges.len(), 1);
    assert_eq!(after_ranges[0].operation, TextOperation::Insert);
    assert_eq!(after_ranges[0].source.end_row, SMALL_CAP + 10);
}

/// The real `PLAIN_TEXT_MAX_EDIT` covers a scattered edit to a 10,000-line file. Cheap: Myers
/// stops at the actual edit distance, not the cap.
#[test]
fn plain_text_line_diff_handles_a_ten_thousand_line_file_with_scattered_changes() {
    let before: String = (0..10_000).map(|i| format!("line-{i}\n")).collect();
    let after: String = (0..10_000)
        .map(|i| {
            if i % 137 == 0 {
                format!("changed-line-{i}\n")
            } else {
                format!("line-{i}\n")
            }
        })
        .collect();
    let (before_ranges, after_ranges) = plain_text_line_diff(&before, &after);

    assert!(
        before_ranges
            .iter()
            .any(|r| r.operation == TextOperation::Delete),
        "a real per-line diff should find the scattered deletes, not give up and replace the \
         whole file: got {} before ranges",
        before_ranges.len()
    );
    assert!(
        after_ranges
            .iter()
            .any(|r| r.operation == TextOperation::Insert),
        "a real per-line diff should find the scattered inserts, not give up and replace the \
         whole file: got {} after ranges",
        after_ranges.len()
    );
    assert!(
        before_ranges.len() > 10,
        "10,000/137 ≈ 73 scattered changes should produce many small ranges, not one giant \
         replaced-whole-file range: got {} before ranges",
        before_ranges.len()
    );
}

#[test]
fn whole_file_class_identical_when_no_lines_changed() {
    assert_eq!(
        whole_file_text_class("a\nb\nc\n", "a\nb\nc\n"),
        WholeFileClass::Identical
    );
}

#[test]
fn whole_file_class_insert_only_when_nothing_deleted() {
    assert_eq!(
        whole_file_text_class("a\nc\n", "a\nb\nc\n"),
        WholeFileClass::InsertOnly
    );
}

#[test]
fn whole_file_class_delete_only_when_nothing_inserted() {
    assert_eq!(
        whole_file_text_class("a\nb\nc\n", "a\nc\n"),
        WholeFileClass::DeleteOnly
    );
}

#[test]
fn whole_file_class_mixed_when_both_inserted_and_deleted() {
    assert_eq!(
        whole_file_text_class("a\nb\nc\n", "a\nx\nc\n"),
        WholeFileClass::Mixed,
        "a changed line is a delete+insert pair, not an Update - so it's Mixed, not licensed"
    );
}

#[test]
fn whole_file_class_mixed_when_myers_lcs_gives_up() {
    const SMALL_CAP: usize = 20;
    let before: String = (0..SMALL_CAP + 10)
        .map(|i| format!("before-unique-line-{i}\n"))
        .collect();
    let after: String = (0..SMALL_CAP + 10)
        .map(|i| format!("after-unique-line-{i}\n"))
        .collect();
    let class = line_diff_core(&before, &after, SMALL_CAP)
        .map(|core| core.whole_file_class())
        .unwrap_or(WholeFileClass::Mixed);
    assert_eq!(
        class,
        WholeFileClass::Mixed,
        "a give-up must never be reported as a licensable class - no license should be \
         granted from an edit distance too large to have actually been measured"
    );
}

/// Cross-checks `whole_file_text_class` against an independent classification (Python
/// `difflib.SequenceMatcher`, `autojunk=False`) of the corpus in
/// `src/test/data/whole_file_text_classification_census.csv`. The class licenses a delete-free or
/// insert-free resolver, so wiring bugs need an external ground truth to catch.
#[test]
#[ignore = "slow"]
fn whole_file_text_class_matches_independent_census() -> Result<()> {
    let census_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("test")
        .join("data")
        .join("whole_file_text_classification_census.csv");
    let census_csv = std::fs::read_to_string(&census_path)?;

    let pairs = test::helper::handmade_test_code_pairs()?;
    let mut mismatches = Vec::new();
    let mut checked = 0;
    for line in census_csv.lines().skip(1) {
        let (fixture, expected_str) = line
            .split_once(',')
            .expect("census CSV row must be `fixture,classification`");
        let expected = match expected_str {
            "Identical" => WholeFileClass::Identical,
            "InsertOnly" => WholeFileClass::InsertOnly,
            "DeleteOnly" => WholeFileClass::DeleteOnly,
            "Mixed" => WholeFileClass::Mixed,
            other => panic!("unknown census classification `{other}` for `{fixture}`"),
        };
        let Some((before, after)) = pairs.get(fixture) else {
            continue;
        };
        checked += 1;
        let actual = whole_file_text_class(&before.contents, &after.contents);
        if actual != expected {
            mismatches.push(format!("{fixture}: census={expected:?} rust={actual:?}"));
        }
    }

    assert!(
        checked > 300,
        "expected to check the vast majority of the 338-fixture corpus, only checked {checked}"
    );
    assert!(
        mismatches.is_empty(),
        "{} whole-file classification mismatches vs the independent census:\n{}",
        mismatches.len(),
        mismatches.join("\n")
    );
    Ok(())
}

/// A row covered by both an `Update` (a changed token) and an `Identical` range (the rest of the
/// row) is `Update`, whichever comes last.
#[test]
fn line_operations_does_not_let_a_same_row_identical_range_hide_a_real_change() {
    let ranges = vec![
        RangeMatch {
            source: TextRange::new(0, 4, 0, 12),
            destination: TextRange::new(0, 4, 0, 12),
            operation: TextOperation::Update,
        },
        // After the `Update` on purpose.
        RangeMatch {
            source: TextRange::new(0, 12, 1, 0),
            destination: TextRange::new(0, 12, 1, 0),
            operation: TextOperation::Identical,
        },
    ];
    assert_eq!(line_operations(&ranges, 1), vec![TextOperation::Update]);
}

#[test]
fn line_operations_treats_a_zero_width_range_as_a_placeholder_not_a_real_row() {
    let ranges = vec![RangeMatch {
        source: TextRange::new(1, 0, 1, 0),
        destination: TextRange::new(1, 0, 2, 0),
        operation: TextOperation::Delete,
    }];
    let ops = line_operations(&ranges, 3);
    assert_eq!(ops, vec![TextOperation::Identical; 3]);
}

/// Row 1 updated on both sides and row 3 inserted: the update counts once, not once per side.
#[test]
fn change_counts_tallies_insertions_deletions_and_updates_without_double_counting() {
    let update_range = |row: usize| RangeMatch {
        source: TextRange::new(row, 0, row, 1),
        destination: TextRange::new(row, 0, row, 1),
        operation: TextOperation::Update,
    };
    let before_ranges = vec![update_range(1)];
    let after_ranges = vec![
        update_range(1),
        RangeMatch {
            source: TextRange::new(3, 0, 3, 1),
            destination: TextRange::new(3, 0, 3, 1),
            operation: TextOperation::Insert,
        },
    ];

    let counts = change_counts("a\nb\nc", "a\nX\nc\nd", &before_ranges, &after_ranges);
    assert_eq!(
        counts,
        ChangeCounts {
            insertions: 1,
            deletions: 0,
            updates: 1,
            moves: 0,
        }
    );
}

/// A moved line exists on both sides, so `Move` counts once, from the after side.
#[test]
fn change_counts_tallies_moves_once_from_the_after_side() {
    let move_range = |row: usize, dest_row: usize| RangeMatch {
        source: TextRange::new(row, 0, row, 1),
        destination: TextRange::new(dest_row, 0, dest_row, 1),
        operation: TextOperation::Move,
    };
    let before_ranges = vec![move_range(0, 2)];
    let after_ranges = vec![move_range(2, 0)];

    let counts = change_counts("a\nb\nc", "b\nc\na", &before_ranges, &after_ranges);
    assert_eq!(
        counts,
        ChangeCounts {
            insertions: 0,
            deletions: 0,
            updates: 0,
            moves: 1,
        }
    );
}

#[test]
fn no_change_all_ranges() -> Result<()> {
    let (before, after) = &*test::helper::handmade_test_code_pair("rust-no-change")?;
    let node_cache = NodeCache::build(before, after);
    let diff = crate::diff::Diff::from_code(before, after);

    let text_diff = TextDiff::from(before, after, &diff.ast.unwrap(), &node_cache);

    let before_ranges = text_diff.all(0);
    assert_eq!(before_ranges.len(), 1, "Wrong number of before ranges");

    let after_ranges = text_diff.all(1);
    assert_eq!(after_ranges.len(), 1, "Wrong number of after ranges");

    assert_eq!(
        before_ranges[0].operation,
        TextOperation::Identical,
        "The identical part has wrong operation"
    );
    assert_eq!(
        before_ranges[0].source.start_row, 0,
        "The identical part has wrong source start row"
    );
    assert_eq!(
        before_ranges[0].source.start_column, 0,
        "The identical part has wrong source start column"
    );
    assert_eq!(
        before_ranges[0].source.end_row, 49,
        "The identical part has wrong source end row"
    );
    assert_eq!(
        before_ranges[0].source.end_column, 0,
        "The identical part has wrong source end column"
    );
    assert_eq!(
        before_ranges[0].destination.start_row, 0,
        "The identical part has wrong destination start row"
    );
    assert_eq!(
        before_ranges[0].destination.start_column, 0,
        "The identical part has wrong destination start column"
    );
    assert_eq!(
        before_ranges[0].destination.end_row, 49,
        "The identical part has wrong destination end row"
    );
    assert_eq!(
        before_ranges[0].destination.end_column, 0,
        "The identical part has wrong destination end column"
    );

    assert_eq!(
        after_ranges[0].operation,
        TextOperation::Identical,
        "When looking from after to before: The identical part has wrong operation"
    );
    assert_eq!(
        after_ranges[0].source.start_row, 0,
        "When looking from after to before: The identical part has wrong source start row"
    );
    assert_eq!(
        after_ranges[0].source.start_column, 0,
        "When looking from after to before: The identical part has wrong source start column"
    );
    assert_eq!(
        after_ranges[0].source.end_row, 49,
        "When looking from after to before: The identical part has wrong source end row"
    );
    assert_eq!(
        after_ranges[0].source.end_column, 0,
        "When looking from after to before: The identical part has wrong source end column"
    );
    assert_eq!(
        after_ranges[0].destination.start_row, 0,
        "When looking from after to before: The identical part has wrong destination start row"
    );
    assert_eq!(
        after_ranges[0].destination.start_column, 0,
        "When looking from after to before: The identical part has wrong destination start column"
    );
    assert_eq!(
        after_ranges[0].destination.end_row, 49,
        "When looking from after to before: The identical part has wrong destination end row"
    );
    assert_eq!(
        after_ranges[0].destination.end_column, 0,
        "When looking from after to before: The identical part has wrong destination end column"
    );

    Ok(())
}

#[test]
fn hello_world_added_message_all_ranges() -> Result<()> {
    let (before, after) =
        &*test::helper::handmade_test_code_pair("rust-hello-world-added-message")?;
    let node_cache = NodeCache::build(before, after);
    let diff = crate::diff::Diff::from_code(before, after);

    let text_diff = TextDiff::from(before, after, &diff.ast.unwrap(), &node_cache);

    let before_ranges = text_diff.all(0);
    assert_eq!(before_ranges.len(), 3, "Wrong number of before ranges");

    assert_eq!(
        before_ranges[0].operation,
        TextOperation::Identical,
        "The initial identical part has wrong operation"
    );
    assert_eq!(
        before_ranges[0].source.start_row, 0,
        "The initial identical part has wrong source start row"
    );
    assert_eq!(
        before_ranges[0].source.start_column, 0,
        "The initial identical part has wrong source start column"
    );
    assert_eq!(
        before_ranges[0].source.end_row, 2,
        "The initial identical part has wrong source end row"
    );
    assert_eq!(
        before_ranges[0].source.end_column, 0,
        "The initial identical part has wrong source end column"
    );
    assert_eq!(
        before_ranges[0].destination.start_row, 0,
        "The initial identical part has wrong destination start row"
    );
    assert_eq!(
        before_ranges[0].destination.start_column, 0,
        "The initial identical part has wrong destination start column"
    );
    assert_eq!(
        before_ranges[0].destination.end_row, 2,
        "The initial identical part has wrong destination end row"
    );
    assert_eq!(
        before_ranges[0].destination.end_column, 0,
        "The initial identical part has wrong destination end column"
    );

    assert_eq!(
        before_ranges[1].operation,
        TextOperation::Delete,
        "The virtual delete, that marks the 'insert' on the after side, has wrong operation"
    );
    assert_eq!(
        before_ranges[1].source.start_row, 2,
        "The virtual delete, that marks the 'insert' on the after side, has wrong source start row"
    );
    assert_eq!(
        before_ranges[1].source.start_column, 0,
        "The virtual delete, that marks the 'insert' on the after side, has wrong source start column"
    );
    assert_eq!(
        before_ranges[1].source.end_row, 2,
        "The virtual delete, that marks the 'insert' on the after side, has wrong source end row"
    );
    assert_eq!(
        before_ranges[1].source.end_column, 0,
        "The virtual delete, that marks the 'insert' on the after side, has wrong source end column"
    );
    // Note that because we ignore whitespace, the [(2, 0), (2, 2)> range is simply missing from
    // the result.
    assert_eq!(
        before_ranges[1].destination.start_row, 2,
        "The virtual delete, that marks the 'insert' on the after side, has wrong destination start row"
    );
    assert_eq!(
        before_ranges[1].destination.start_column, 2,
        "The virtual delete, that marks the 'insert' on the after side, has wrong destination start column"
    );
    assert_eq!(
        before_ranges[1].destination.end_row, 3,
        "The virtual delete, that marks the 'insert' on the after side, has wrong destination end row"
    );
    assert_eq!(
        before_ranges[1].destination.end_column, 0,
        "The virtual delete, that marks the 'insert' on the after side, has wrong destination end column"
    );

    assert_eq!(
        before_ranges[2].operation,
        TextOperation::Identical,
        "The final identical part has wrong operation"
    );
    assert_eq!(
        before_ranges[2].source.start_row, 2,
        "The final identical part has wrong source start row"
    );
    assert_eq!(
        before_ranges[2].source.start_column, 0,
        "The final identical part has wrong source start column"
    );
    assert_eq!(
        before_ranges[2].source.end_row, 3,
        "The final identical part has wrong source end row"
    );
    assert_eq!(
        before_ranges[2].source.end_column, 0,
        "The final identical part has wrong source end column"
    );
    assert_eq!(
        before_ranges[2].destination.start_row, 3,
        "The final identical part has wrong destination start row"
    );
    assert_eq!(
        before_ranges[2].destination.start_column, 0,
        "The final identical part has wrong destination start column"
    );
    assert_eq!(
        before_ranges[2].destination.end_row, 4,
        "The final identical part has wrong destination end row"
    );
    assert_eq!(
        before_ranges[2].destination.end_column, 0,
        "The final identical part has wrong destination end column"
    );

    let after_ranges = text_diff.all(1);
    assert_eq!(
        after_ranges.len(),
        3,
        "When looking from after to before: Wrong number of after ranges"
    );

    assert_eq!(
        after_ranges[0].operation,
        TextOperation::Identical,
        "When looking from after to before: The initial identical part has wrong operation"
    );
    assert_eq!(
        after_ranges[0].source.start_row, 0,
        "When looking from after to before: The initial identical part has wrong source start row"
    );
    assert_eq!(
        after_ranges[0].source.start_column, 0,
        "When looking from after to before: The initial identical part has wrong source start column"
    );
    assert_eq!(
        after_ranges[0].source.end_row, 2,
        "When looking from after to before: The initial identical part has wrong source end row"
    );
    assert_eq!(
        after_ranges[0].source.end_column, 0,
        "When looking from after to before: The initial identical part has wrong source end column"
    );
    assert_eq!(
        after_ranges[0].destination.start_row, 0,
        "When looking from after to before: The initial identical part has wrong destination start row"
    );
    assert_eq!(
        after_ranges[0].destination.start_column, 0,
        "When looking from after to before: The initial identical part has wrong destination start column"
    );
    assert_eq!(
        after_ranges[0].destination.end_row, 2,
        "When looking from after to before: The initial identical part has wrong destination end row"
    );
    assert_eq!(
        after_ranges[0].destination.end_column, 0,
        "When looking from after to before: The initial identical part has wrong destination end column"
    );

    assert_eq!(
        after_ranges[1].operation,
        TextOperation::Insert,
        "When looking from after to before: The insert has the wrong operation"
    );
    assert_eq!(
        after_ranges[1].source.start_row, 2,
        "When looking from after to before: The insert has wrong source start row"
    );
    assert_eq!(
        after_ranges[1].source.start_column, 2,
        "When looking from after to before: The insert has wrong source start column"
    );
    assert_eq!(
        after_ranges[1].source.end_row, 3,
        "When looking from after to before: The insert has wrong source end row"
    );
    assert_eq!(
        after_ranges[1].source.end_column, 0,
        "When looking from after to before: The insert has wrong source end column"
    );
    assert_eq!(
        after_ranges[1].destination.start_row, 2,
        "When looking from after to before: The insert has wrong destination start row"
    );
    assert_eq!(
        after_ranges[1].destination.start_column, 0,
        "When looking from after to before: The insert has wrong destination start column"
    );
    assert_eq!(
        after_ranges[1].destination.end_row, 2,
        "When looking from after to before: The insert has wrong destination end row"
    );
    assert_eq!(
        after_ranges[1].destination.end_column, 0,
        "When looking from after to before: The insert has wrong destination end column"
    );

    assert_eq!(
        after_ranges[2].operation,
        TextOperation::Identical,
        "When looking from after to before: The final identical part has wrong operation"
    );
    assert_eq!(
        after_ranges[2].source.start_row, 3,
        "When looking from after to before: The final identical part has wrong source start row"
    );
    assert_eq!(
        after_ranges[2].source.start_column, 0,
        "When looking from after to before: The final identical part has wrong source start column"
    );
    assert_eq!(
        after_ranges[2].source.end_row, 4,
        "When looking from after to before: The final identical part has wrong source end row"
    );
    assert_eq!(
        after_ranges[2].source.end_column, 0,
        "When looking from after to before: The final identical part has wrong source end column"
    );
    assert_eq!(
        after_ranges[2].destination.start_row, 2,
        "When looking from after to before: The final identical part has wrong destination start row"
    );
    assert_eq!(
        after_ranges[2].destination.start_column, 0,
        "When looking from after to before: The final identical part has wrong destination start column"
    );
    assert_eq!(
        after_ranges[2].destination.end_row, 3,
        "When looking from after to before: The final identical part has wrong destination end row"
    );
    assert_eq!(
        after_ranges[2].destination.end_column, 0,
        "When looking from after to before: The final identical part has wrong destination end column"
    );

    Ok(())
}

#[test]
fn python_leetcode_1_added_if_block_all_ranges() -> Result<()> {
    let (before, after) = &*test::helper::handmade_test_code_pair("python-added-if-block")?;
    let node_cache = NodeCache::build(before, after);
    let diff = crate::diff::Diff::from_code(before, after);

    let text_diff = TextDiff::from(before, after, &diff.ast.unwrap(), &node_cache);

    let before_ranges = text_diff.all(0);
    assert_eq!(before_ranges.len(), 3);

    assert_eq!(before_ranges[0].operation, TextOperation::Identical);
    assert_eq!(before_ranges[0].source.start_row, 0);
    assert_eq!(before_ranges[0].source.start_column, 0);
    assert_eq!(before_ranges[0].source.end_row, 20);
    assert_eq!(before_ranges[0].source.end_column, 0);
    assert_eq!(before_ranges[0].destination.start_row, 0);
    assert_eq!(before_ranges[0].destination.start_column, 0);
    assert_eq!(before_ranges[0].destination.end_row, 20);
    assert_eq!(before_ranges[0].destination.end_column, 0);

    // This is a "empty range" that indicates something exists here in the other side.
    // Note that because we ignore whitespace, the leading 4-space indentation of the new
    // "if" line is simply missing from the result, and the destination starts at column 4.
    assert_eq!(before_ranges[1].operation, TextOperation::Delete);
    assert_eq!(before_ranges[1].source.start_row, 20);
    assert_eq!(before_ranges[1].source.start_column, 0);
    assert_eq!(before_ranges[1].source.end_row, 20);
    assert_eq!(before_ranges[1].source.end_column, 0);
    assert_eq!(before_ranges[1].destination.start_row, 20);
    assert_eq!(before_ranges[1].destination.start_column, 4);
    assert_eq!(before_ranges[1].destination.end_row, 21);
    assert_eq!(before_ranges[1].destination.end_column, 0);

    // Note the order between the empty range and the actual range that exists. The empty range
    // must always be before an actual existing range, even if their start point is equal.
    // This is the print statement that was re-indented (column 4 -> column 8) because it now
    // lives one level deeper inside the new "if" block. Its text is identical, but its
    // position moved, so it's a Move rather than an Identical range.
    assert_eq!(before_ranges[2].operation, TextOperation::Move);
    assert_eq!(before_ranges[2].source.start_row, 20);
    assert_eq!(before_ranges[2].source.start_column, 4);
    assert_eq!(before_ranges[2].source.end_row, 21);
    assert_eq!(before_ranges[2].source.end_column, 0);
    assert_eq!(before_ranges[2].destination.start_row, 21);
    assert_eq!(before_ranges[2].destination.start_column, 8);
    assert_eq!(before_ranges[2].destination.end_row, 22);
    assert_eq!(before_ranges[2].destination.end_column, 0);

    let after_ranges = text_diff.all(1);
    // Note the symmetric relationships between source and destination ranges in the
    // before_ranges and after_ranges vectors.
    assert_eq!(after_ranges.len(), before_ranges.len());

    assert_eq!(after_ranges[0].operation, TextOperation::Identical);
    assert_eq!(after_ranges[0].source.start_row, 0);
    assert_eq!(after_ranges[0].source.start_column, 0);
    assert_eq!(after_ranges[0].source.end_row, 20);
    assert_eq!(after_ranges[0].source.end_column, 0);
    assert_eq!(after_ranges[0].destination.start_row, 0);
    assert_eq!(after_ranges[0].destination.start_column, 0);
    assert_eq!(after_ranges[0].destination.end_row, 20);
    assert_eq!(after_ranges[0].destination.end_column, 0);

    // The added "if" conditional (leading 4-space indentation ignored, same as above).
    assert_eq!(after_ranges[1].operation, TextOperation::Insert);
    assert_eq!(after_ranges[1].source.start_row, 20);
    assert_eq!(after_ranges[1].source.start_column, 4);
    assert_eq!(after_ranges[1].source.end_row, 21);
    assert_eq!(after_ranges[1].source.end_column, 0);
    assert_eq!(after_ranges[1].destination.start_row, 20);
    assert_eq!(after_ranges[1].destination.start_column, 0);
    assert_eq!(after_ranges[1].destination.end_row, 20);
    assert_eq!(after_ranges[1].destination.end_column, 0);

    // The matched existing implementation, moved one level deeper.
    assert_eq!(after_ranges[2].operation, TextOperation::Move);
    assert_eq!(after_ranges[2].source.start_row, 21);
    assert_eq!(after_ranges[2].source.start_column, 8);
    assert_eq!(after_ranges[2].source.end_row, 22);
    assert_eq!(after_ranges[2].source.end_column, 0);
    assert_eq!(after_ranges[2].destination.start_row, 20);
    assert_eq!(after_ranges[2].destination.start_column, 4);
    assert_eq!(after_ranges[2].destination.end_row, 21);
    assert_eq!(after_ranges[2].destination.end_column, 0);

    Ok(())
}

fn range(operation: TextOperation) -> RangeMatch {
    RangeMatch {
        source: TextRange::new(0, 0, 1, 0),
        destination: TextRange::new(0, 0, 1, 0),
        operation,
    }
}

#[test]
fn whitespace_stripped_equal_ignores_all_whitespace_differences() {
    assert!(whitespace_stripped_equal(
        "fn main() {\n    foo();\n}\n",
        "fn main(){foo();}"
    ));
    assert!(!whitespace_stripped_equal("fn main() {}", "fn other() {}"));
}

#[test]
fn summarize_diff_is_no_changes_when_every_range_is_identical() {
    let ranges = vec![range(TextOperation::Identical)];
    assert_eq!(
        summarize_diff("same", "same", &ranges, &ranges),
        Some(DiffSummary::NoChanges)
    );
}

#[test]
fn summarize_diff_is_no_changes_for_two_empty_files() {
    assert_eq!(
        summarize_diff("", "", &[], &[]),
        Some(DiffSummary::NoChanges)
    );
}

#[test]
fn summarize_diff_is_new_file_when_only_inserts_are_present() {
    let before_ranges: Vec<RangeMatch> = vec![];
    let after_ranges = vec![range(TextOperation::Insert)];
    assert_eq!(
        summarize_diff("", "fn main() {}", &before_ranges, &after_ranges),
        Some(DiffSummary::NewFile)
    );
}

/// One line added to an otherwise unchanged file is not `NewFile`: its `Identical` ranges
/// disqualify it.
#[test]
fn summarize_diff_is_not_new_file_when_inserts_are_mixed_with_identical_content() {
    let ranges = vec![
        range(TextOperation::Identical),
        range(TextOperation::Insert),
    ];
    assert_eq!(
        summarize_diff(
            "fn main() {\n    foo();\n}",
            "fn main() {\n    foo();\n    bar();\n}",
            &ranges,
            &ranges
        ),
        None
    );
}

#[test]
fn summarize_diff_is_not_deleted_file_when_deletes_are_mixed_with_identical_content() {
    let ranges = vec![
        range(TextOperation::Identical),
        range(TextOperation::Delete),
    ];
    assert_eq!(
        summarize_diff(
            "fn main() {\n    foo();\n    bar();\n}",
            "fn main() {\n    foo();\n}",
            &ranges,
            &ranges
        ),
        None
    );
}

#[test]
fn summarize_diff_is_deleted_file_when_only_deletes_are_present() {
    let before_ranges = vec![range(TextOperation::Delete)];
    let after_ranges: Vec<RangeMatch> = vec![];
    assert_eq!(
        summarize_diff("fn main() {}", "", &before_ranges, &after_ranges),
        Some(DiffSummary::DeletedFile)
    );
}

#[test]
fn summarize_diff_is_whitespace_only_when_stripped_content_matches_despite_move_ranges() {
    // A pure reindent paints `Move`s; only the stripped content says nothing changed.
    let ranges = vec![range(TextOperation::Move)];
    assert_eq!(
        summarize_diff(
            "fn main() {\nfoo();\n}",
            "fn main() {\n    foo();\n}",
            &ranges,
            &ranges
        ),
        Some(DiffSummary::WhitespaceOnly)
    );
}

#[test]
fn summarize_diff_is_refactor_moved_only_when_only_moves_are_present_and_content_really_differs() {
    let ranges = vec![range(TextOperation::Move)];
    assert_eq!(
        summarize_diff(
            "fn a() {}\nfn b() {}",
            "fn b() {}\nfn a() {}",
            &ranges,
            &ranges
        ),
        Some(DiffSummary::RefactorMovedOnly)
    );
}

#[test]
fn summarize_diff_is_none_for_a_genuine_mixed_edit() {
    let ranges = vec![range(TextOperation::Update), range(TextOperation::Insert)];
    assert_eq!(summarize_diff("a", "b", &ranges, &ranges), None);
}

#[test]
fn summarize_diff_prefers_whitespace_only_over_refactor_when_both_could_apply() {
    // Only `Move`s and whitespace-stripped-equal content: "reformatted" is the more specific claim.
    let ranges = vec![range(TextOperation::Move)];
    assert_eq!(
        summarize_diff("same", " same ", &ranges, &ranges),
        Some(DiffSummary::WhitespaceOnly)
    );
}

/// A reformat matched as one subtree whose start did not move produces no operations at all, and
/// is still `WhitespaceOnly`, not `NoChanges`: the files are not byte-identical.
#[test]
fn summarize_diff_is_whitespace_only_even_with_zero_operations_when_content_is_not_byte_identical()
{
    let no_ranges: Vec<RangeMatch> = vec![];
    assert_eq!(
        summarize_diff(
            "fn main() {\nfoo();\n}",
            "fn main() {\n    foo();\n}",
            &no_ranges,
            &no_ranges,
        ),
        Some(DiffSummary::WhitespaceOnly)
    );
}

/// A real parse and diff: `is_comment_only_diff` reads node kinds.
fn diff_ast(
    before_src: &str,
    after_src: &str,
) -> (crate::code::Code, crate::code::Code, ASTDiff, NodeCache) {
    let before = crate::code::Code::from_string(before_src, &crate::code::Language::Rust);
    let after = crate::code::Code::from_string(after_src, &crate::code::Language::Rust);
    let node_cache = NodeCache::build(&before, &after);
    let diff = crate::diff::diff_code(&before, &after);
    let ast = diff
        .ast
        .expect("diff_code should always produce an AST for valid Rust");
    (before, after, ast, node_cache)
}

/// A new `line_comment` is a `//` leaf plus its own words; the words are painted too, not just the
/// marker (`rust-cost-optimization`).
#[test]
fn ranges_paints_a_wholly_new_comments_own_words_not_just_its_marker() {
    let (before, after, ast, node_cache) =
        diff_ast("fn main() {}\n", "// hi there\nfn main() {}\n");
    let after_ranges = ranges(
        &after,
        &before,
        &ast,
        &node_cache,
        false,
        RenderOptions::FULL,
    );

    let words_painted = after_ranges.iter().any(|r| {
        r.operation == TextOperation::Insert
            && r.source.start_row == 0
            && r.source.start_column >= 2 // past the `//` marker itself
            && (r.source.end_row > r.source.start_row || r.source.end_column > r.source.start_column)
    });
    assert!(
        words_painted,
        "a brand-new comment's own words (everything after `//`) must be painted Insert, not \
         silently dropped: {after_ranges:?}"
    );
}

/// A string literal its children fully cover (`"` + content + `"`) descends normally, so the
/// whitespace between it and its siblings still merges: a new `let s = "a" + b;` is one `Insert`
/// end to end, not split around the literal (`java-add-logging`).
#[test]
fn a_no_gap_string_literal_does_not_break_whitespace_merging_with_a_sibling() {
    let (before, after, ast, node_cache) =
        diff_ast("fn main() {}\n", "fn main() {\n    let s = \"a\" + b;\n}\n");
    let after_ranges = ranges(
        &after,
        &before,
        &ast,
        &node_cache,
        false,
        RenderOptions::FULL,
    );

    let row1_inserts: Vec<_> = after_ranges
        .iter()
        .filter(|r| r.operation == TextOperation::Insert && r.source.start_row == 1)
        .collect();
    assert_eq!(
        row1_inserts.len(),
        1,
        "the whole new `let s = \"a\" + b;` statement should merge into one Insert range - \
         more than one means a gap (like the whitespace around `+`) split it apart: \
         {after_ranges:?}"
    );
    assert_eq!(
        row1_inserts[0].source.start_column, 4,
        "the merged Insert range should start at `let`, right after the line's indentation: \
         {after_ranges:?}"
    );
}
/// Swapping two sibling functions (same column, different rows) paints `Move`, not nothing
/// (`crossed_backwards`).
#[test]
fn sibling_reorder_produces_move_ranges_and_a_refactor_moved_summary() {
    let before_src = "fn main() {\n    let a = 1;\n    println!(\"{}\", a);\n}\n\nfn helper(x: i32) -> i32 {\n    x * 2\n}\n";
    let after_src = "fn helper(x: i32) -> i32 {\n    x * 2\n}\n\nfn main() {\n    let a = 1;\n    println!(\"{}\", a);\n}\n";
    let (before, after, ast, node_cache) = diff_ast(before_src, after_src);
    let text_diff = TextDiff::from(&before, &after, &ast, &node_cache);
    let before_ranges = text_diff.all(0);
    let after_ranges = text_diff.all(1);

    for (side, ranges) in [("before", &before_ranges), ("after", &after_ranges)] {
        assert!(
            ranges.iter().any(|r| r.operation == TextOperation::Move),
            "{side} side should contain a Move range for a sibling reorder, got {ranges:?}"
        );
        assert!(
            ranges
                .iter()
                .all(|r| matches!(r.operation, TextOperation::Move | TextOperation::Identical)),
            "{side} side of a pure reorder should only have Move/Identical ranges, got {ranges:?}"
        );
    }
    assert_eq!(
        summarize_diff(before_src, after_src, &before_ranges, &after_ranges),
        Some(DiffSummary::RefactorMovedOnly)
    );
}

/// Content shifted down by an insertion above keeps its column and order, so stays `Identical`.
#[test]
fn unrelated_insertion_does_not_flag_shifted_content_as_moved() {
    let before_src = "fn main() {\n    foo();\n}\n\nfn helper() {\n    bar();\n}\n";
    let after_src = "fn added() {}\n\nfn main() {\n    foo();\n}\n\nfn helper() {\n    bar();\n}\n";
    let (before, after, ast, node_cache) = diff_ast(before_src, after_src);
    let text_diff = TextDiff::from(&before, &after, &ast, &node_cache);
    for (side, ranges) in [("before", &text_diff.all(0)), ("after", &text_diff.all(1))] {
        assert!(
            !ranges.iter().any(|r| r.operation == TextOperation::Move),
            "{side} side should have no Move ranges when content merely shifted down, got {ranges:?}"
        );
    }
}

/// A changed comment maps `MatchButNotIdentical`, not `Update`, and still counts as a change, in
/// agreement with what `ranges` paints.
#[test]
fn is_comment_only_diff_is_true_when_only_a_comments_text_changed() {
    let (before, after, ast, node_cache) = diff_ast(
        "// old comment\nfn main() {}",
        "// new comment\nfn main() {}",
    );
    assert!(is_comment_only_diff(&before, &after, &ast, &node_cache));
}

#[test]
fn is_comment_only_diff_is_true_when_a_comment_was_inserted() {
    let (before, after, ast, node_cache) = diff_ast("fn main() {}", "// a comment\nfn main() {}");
    assert!(is_comment_only_diff(&before, &after, &ast, &node_cache));
}

#[test]
fn is_comment_only_diff_is_true_when_a_comment_was_deleted() {
    let (before, after, ast, node_cache) = diff_ast("// a comment\nfn main() {}", "fn main() {}");
    assert!(is_comment_only_diff(&before, &after, &ast, &node_cache));
}

#[test]
fn is_comment_only_diff_is_false_for_a_real_code_change() {
    let (before, after, ast, node_cache) = diff_ast("fn main() { old(); }", "fn main() { new(); }");
    assert!(!is_comment_only_diff(&before, &after, &ast, &node_cache));
}

#[test]
fn is_comment_only_diff_is_false_when_a_comment_and_real_code_both_changed() {
    let (before, after, ast, node_cache) = diff_ast(
        "// old comment\nfn main() { old(); }",
        "// new comment\nfn main() { new(); }",
    );
    assert!(!is_comment_only_diff(&before, &after, &ast, &node_cache));
}

#[test]
fn is_comment_only_diff_is_false_when_nothing_changed_at_all() {
    // "Comment-only" is a claim about what changed; with nothing changed it is false.
    let (before, after, ast, node_cache) = diff_ast("fn main() {}", "fn main() {}");
    assert!(!is_comment_only_diff(&before, &after, &ast, &node_cache));
}

/// A statement wrapped one level deeper stays `Identical` in the AST, but its enclosing block
/// changed, so the diff is not comment-only.
#[test]
fn is_comment_only_diff_is_false_when_a_statement_moves_one_level_deeper() {
    let (before, after, ast, node_cache) = diff_ast(
        "fn main() {\n    foo();\n    bar();\n}",
        "fn main() {\n    foo();\n    if true {\n        bar();\n    }\n}",
    );
    assert!(!is_comment_only_diff(&before, &after, &ast, &node_cache));
}

#[test]
fn summarize_diff_with_comment_check_reports_comment_only_over_no_classification() {
    let ranges = vec![range(TextOperation::Update)];
    assert_eq!(
        summarize_diff_with_comment_check("// a", "// b", &ranges, &ranges, true),
        Some(DiffSummary::CommentOnly)
    );
}

#[test]
fn summarize_diff_with_comment_check_reports_comment_only_over_refactor_moved_only() {
    let ranges = vec![range(TextOperation::Move)];
    assert_eq!(
        summarize_diff_with_comment_check(
            "fn a() {}\nfn b() {}",
            "fn b() {}\nfn a() {}",
            &ranges,
            &ranges,
            true,
        ),
        Some(DiffSummary::CommentOnly)
    );
}

#[test]
fn summarize_diff_with_comment_check_does_not_override_new_file() {
    let before_ranges: Vec<RangeMatch> = vec![];
    let after_ranges = vec![range(TextOperation::Insert)];
    assert_eq!(
        summarize_diff_with_comment_check(
            "",
            "// just a comment",
            &before_ranges,
            &after_ranges,
            true,
        ),
        Some(DiffSummary::NewFile),
        "a wholly new file should stay NewFile even if it's all comments"
    );
}

#[test]
fn summarize_diff_with_comment_check_ignores_the_flag_when_false() {
    let ranges = vec![range(TextOperation::Move)];
    assert_eq!(
        summarize_diff_with_comment_check(
            "fn a() {}\nfn b() {}",
            "fn b() {}\nfn a() {}",
            &ranges,
            &ranges,
            false,
        ),
        Some(DiffSummary::RefactorMovedOnly)
    );
}

/// Under `MINIMAL`, one changed character inside a long identifier is one narrow `Update`.
#[test]
fn ranges_decomposes_a_small_change_inside_a_long_identifier() {
    let (before, after, ast, node_cache) = diff_ast(
        "fn main() {\n    let long_identifier_name = 5;\n}",
        "fn main() {\n    let long_identifier_nome = 5;\n}",
    );
    let before_ranges = ranges(
        &before,
        &after,
        &ast,
        &node_cache,
        true,
        RenderOptions::MINIMAL,
    );

    let updates: Vec<_> = before_ranges
        .iter()
        .filter(|r| r.operation == TextOperation::Update)
        .collect();
    assert_eq!(
        updates.len(),
        1,
        "expected exactly one Update sub-range, got {before_ranges:?}"
    );
    let update = updates[0];
    assert_eq!(update.source.start_row, 1);
    assert_eq!(update.source.end_row, 1);
    assert_eq!(
        update.source.end_column - update.source.start_column,
        1,
        "the Update range should cover only the single changed character, not the whole \
         20-character identifier"
    );

    let after_ranges = ranges(
        &after,
        &before,
        &ast,
        &node_cache,
        false,
        RenderOptions::MINIMAL,
    );
    let after_updates: Vec<_> = after_ranges
        .iter()
        .filter(|r| r.operation == TextOperation::Update)
        .collect();
    assert_eq!(after_updates.len(), 1);
    assert_eq!(
        after_updates[0].source.end_column - after_updates[0].source.start_column,
        1,
        "the after->before direction must independently find the same narrow width"
    );
}

/// Under `FULL`, the rename from the test above is painted whole on both sides - ground-truth
/// invariant 16's `Full` half, via [`RenderOptions::whole_identifier_updates`].
#[test]
fn full_paints_a_renamed_identifier_whole() {
    let (before, after, ast, node_cache) = diff_ast(
        "fn main() {\n    let long_identifier_name = 5;\n}",
        "fn main() {\n    let long_identifier_nome = 5;\n}",
    );
    for (source, destination, source_is_before) in
        [(&before, &after, true), (&after, &before, false)]
    {
        let updates: Vec<_> = ranges(
            source,
            destination,
            &ast,
            &node_cache,
            source_is_before,
            RenderOptions::FULL,
        )
        .into_iter()
        .filter(|r| r.operation == TextOperation::Update)
        .collect();
        assert_eq!(updates.len(), 1, "expected one Update, got {updates:?}");
        assert_eq!(
            updates[0].source.end_column - updates[0].source.start_column,
            "long_identifier_name".len(),
            "FULL should paint the whole renamed identifier"
        );
    }
}

/// `whole_identifier_updates` is scoped to identifiers: a changed comment stays narrow under
/// `FULL`.
#[test]
fn full_keeps_a_changed_comment_narrow() {
    let (before, after, ast, node_cache) = diff_ast(
        "// Copyright 2025 Example\nfn main() {}\n",
        "// Copyright 2026 Example\nfn main() {}\n",
    );
    let updates: Vec<_> = ranges(
        &before,
        &after,
        &ast,
        &node_cache,
        true,
        RenderOptions::FULL,
    )
    .into_iter()
    .filter(|r| r.operation == TextOperation::Update)
    .collect();
    assert_eq!(updates.len(), 1, "expected one Update, got {updates:?}");
    assert!(
        updates[0].source.end_column - updates[0].source.start_column <= 2,
        "only the changed digit(s) of the year should be painted, got {:?}",
        updates[0]
    );
}

/// [`RenderOptions::whole_pair_updates`] reports the same edit as one `Update` over the whole
/// identifier.
#[test]
fn ranges_reports_the_whole_identifier_when_whole_pair_updates_is_set() {
    let (before, after, ast, node_cache) = diff_ast(
        "fn main() {\n    let long_identifier_name = 5;\n}",
        "fn main() {\n    let long_identifier_nome = 5;\n}",
    );
    let before_ranges = ranges(
        &before,
        &after,
        &ast,
        &node_cache,
        true,
        RenderOptions {
            whole_pair_updates: true,
            ..RenderOptions::FULL
        },
    );

    let updates: Vec<_> = before_ranges
        .iter()
        .filter(|r| r.operation == TextOperation::Update)
        .collect();
    assert_eq!(
        updates.len(),
        1,
        "expected exactly one Update sub-range, got {before_ranges:?}"
    );
    assert_eq!(
        updates[0].source.end_column - updates[0].source.start_column,
        "long_identifier_name".len(),
        "the Update range should cover the whole identifier, not just the changed character"
    );
}

/// With no common prefix or suffix, the whole span is the `Update`.
#[test]
fn ranges_falls_back_to_a_whole_span_update_when_there_is_no_common_affix() {
    let (before, after, ast, node_cache) = diff_ast(
        "fn main() {\n    let foo = 5;\n}",
        "fn main() {\n    let bar = 5;\n}",
    );
    let before_ranges = ranges(
        &before,
        &after,
        &ast,
        &node_cache,
        true,
        RenderOptions::FULL,
    );

    let updates: Vec<_> = before_ranges
        .iter()
        .filter(|r| r.operation == TextOperation::Update)
        .collect();
    assert_eq!(updates.len(), 1);
    assert_eq!(
        updates[0].source.end_column - updates[0].source.start_column,
        3,
        "\"foo\" and \"bar\" share no common prefix/suffix, so the whole 3-character \
         identifier should be reported as changed"
    );
}

/// A comment's words sit in one gap after its `//` child, so a small change in them splits like an
/// identifier's.
#[test]
fn ranges_decomposes_a_small_change_inside_a_comment() {
    let (before, after, ast, node_cache) = diff_ast(
        "// hello world!\nfn main() {}",
        "// hello universe!\nfn main() {}",
    );
    let before_ranges = ranges(
        &before,
        &after,
        &ast,
        &node_cache,
        true,
        RenderOptions::FULL,
    );

    let updates: Vec<_> = before_ranges
        .iter()
        .filter(|r| r.operation == TextOperation::Update)
        .collect();
    assert_eq!(
        updates.len(),
        1,
        "expected exactly one Update sub-range inside the comment, got {before_ranges:?}"
    );
    assert_eq!(
        updates[0].source.end_column - updates[0].source.start_column,
        5,
        "\"world\" (5 chars) should be the only part marked changed, not the whole comment"
    );

    let identical_in_comment: Vec<_> = before_ranges
        .iter()
        .filter(|r| {
            r.operation == TextOperation::Identical
                && r.source.start_row == 0
                && r.source.end_row == 0
        })
        .collect();
    assert!(
        !identical_in_comment.is_empty(),
        "the common \"// hello \" prefix should be reported as Identical, not swallowed into \
         the Update: {before_ranges:?}"
    );
}

/// An unrelated earlier insertion shifts the changed identifier to different columns on each side.
/// The decomposed ranges bypass merging, so both sides still carry the narrow `Update` and
/// `merge_ranges` stays aligned.
#[test]
fn ranges_decomposition_survives_an_unrelated_earlier_insertion() {
    let (before, after, ast, node_cache) = diff_ast(
        "fn main() {\n    let short = 1;\n    let long_identifier_name = 5;\n}",
        "fn main() {\n    let inserted_line = 0;\n    let short = 1;\n    \
         let long_identifier_nome = 5;\n}",
    );

    // `MINIMAL`: the narrow split is what this guards, and `FULL` paints the rename whole.
    let text_diff =
        TextDiff::from_with_options(&before, &after, &ast, &node_cache, RenderOptions::MINIMAL);
    let before_ranges = text_diff.all(0);
    let after_ranges = text_diff.all(1);

    let before_updates: Vec<_> = before_ranges
        .iter()
        .filter(|r| r.operation == TextOperation::Update)
        .collect();
    let after_updates: Vec<_> = after_ranges
        .iter()
        .filter(|r| r.operation == TextOperation::Update)
        .collect();
    assert_eq!(before_updates.len(), 1, "{before_ranges:?}");
    assert_eq!(after_updates.len(), 1, "{after_ranges:?}");
    assert_eq!(
        before_updates[0].source.end_column - before_updates[0].source.start_column,
        1
    );
    assert_eq!(
        after_updates[0].source.end_column - after_updates[0].source.start_column,
        1
    );
}
/// A phrase appended inside a string literal is an `Insert`: one side's middle is empty, so
/// nothing was replaced.
#[test]
fn a_phrase_added_inside_a_string_renders_as_an_insert_not_an_update() {
    let before = Code::from_string(
        "let s = \"Fetch user data\";\n",
        &crate::code::Language::Rust,
    );
    let after = Code::from_string(
        "let s = \"Fetch user data from API\";\n",
        &crate::code::Language::Rust,
    );
    let diff = crate::diff::diff_code(&before, &after);
    let ast = diff.ast.as_ref().expect("an AST diff");
    let node_cache = NodeCache::build(&before, &after);
    let text_diff = TextDiff::from(&before, &after, ast, &node_cache);

    let ops: Vec<TextOperation> = text_diff
        .all(1)
        .iter()
        .filter(|r| !r.source.is_empty())
        .map(|r| r.operation.clone())
        .collect();
    assert!(
        ops.contains(&TextOperation::Insert),
        "the added words should read as an insertion, got {ops:?}"
    );
    assert!(
        !ops.contains(&TextOperation::Update),
        "and nothing here was replaced, so nothing should be an update: {ops:?}"
    );
}

/// `IntBox` -> `Box` leaves an empty middle too, but an identifier is a name: painters call it a
/// rename, not the deletion of `Int`.
#[test]
fn a_renamed_identifier_stays_an_update_even_though_one_side_is_empty() {
    let before = Code::from_string("struct IntBox;\n", &crate::code::Language::Rust);
    let after = Code::from_string("struct Box;\n", &crate::code::Language::Rust);
    let diff = crate::diff::diff_code(&before, &after);
    let ast = diff.ast.as_ref().expect("an AST diff");
    let node_cache = NodeCache::build(&before, &after);
    let text_diff = TextDiff::from(&before, &after, ast, &node_cache);

    let ops: Vec<TextOperation> = text_diff
        .all(0)
        .iter()
        .filter(|r| !r.source.is_empty())
        .map(|r| r.operation.clone())
        .collect();
    assert!(
        !ops.contains(&TextOperation::Delete),
        "a rename is not a deletion of the dropped prefix: {ops:?}"
    );
}

/// A row carrying two edits, one on each side of an untouched node (`cpp-add-const-correctness`).
/// The node is in neither common affix; `node_uniquely_placed_on_its_row` reads it as untouched
/// because its text occurs once on each row, at its own columns. `FULL` still paints it.
#[test]
fn paint_displaced_moves_gates_a_node_between_two_edits_on_its_own_row() {
    let before = Code::from_string(
        "void printVector(std::vector<int> vec) {\n}\n",
        &crate::code::Language::CPP,
    );
    let after = Code::from_string(
        "void printVector(const std::vector<int>& vec) {\n}\n",
        &crate::code::Language::CPP,
    );
    let ast = crate::diff::diff_code(&before, &after);
    let node_cache = crate::diff::NodeCache::build(&before, &after);

    let type_is_identical = |options| {
        TextDiff::from_with_options(
            &before,
            &after,
            ast.ast.as_ref().unwrap(),
            &node_cache,
            options,
        )
        .all(0)
        .iter()
        .any(|r| {
            r.operation == TextOperation::Identical
                && r.source.start_row == 0
                && r.source.start_column == 17
        })
    };

    assert!(
        type_is_identical(RenderOptions::MINIMAL),
        "the type neither edit touched must not paint Move under MINIMAL"
    );
    assert!(
        !type_is_identical(RenderOptions::FULL),
        "FULL still paints a displaced node, the same split paint_displaced_moves already makes"
    );
}

/// A row rewritten around its one surviving fragment (`typescript-async-await`): `function
/// fetchData(` is unique on both rows, but there is no common suffix, so it still paints `Move`.
#[test]
fn a_row_rewritten_around_its_one_surviving_fragment_still_paints_move() {
    let before = Code::from_string(
        "function fetchData(callback: (data: string) => void): void {\n    return;\n}\n",
        &crate::code::Language::TypeScript,
    );
    let after = Code::from_string(
        "async function fetchData(): Promise<string> {\n    return;\n}\n",
        &crate::code::Language::TypeScript,
    );
    let ast = crate::diff::diff_code(&before, &after);
    let node_cache = crate::diff::NodeCache::build(&before, &after);

    assert!(
        !TextDiff::from_with_options(
            &before,
            &after,
            ast.ast.as_ref().unwrap(),
            &node_cache,
            RenderOptions::MINIMAL,
        )
        .all(0)
        .iter()
        .any(|r| {
            r.operation == TextOperation::Identical
                && r.source.start_row == 0
                && r.source.start_column == 0
        }),
        "a fragment of a rewritten row is relocated, not displaced"
    );
}

/// Text that repeats on its row declines: "it stayed" and "it is the other one" both fit.
#[test]
fn a_node_whose_text_repeats_on_its_row_is_not_read_as_displaced() {
    let source_row: Vec<char> = "let a = count + count + one;".chars().collect();
    let destination_row: Vec<char> = "let a = count + two + count + one;".chars().collect();
    // The first `count`, at its own columns on both sides.
    let s = text_range_on(0, 8, 13);
    let d = text_range_on(0, 8, 13);
    assert!(!node_uniquely_placed_on_its_row(
        &source_row,
        &destination_row,
        &s,
        &d
    ));
}

/// `é` and `è` share their first UTF-8 byte, but no character: the common affixes stay on char
/// boundaries of both strings, so slicing at them never panics.
#[test]
fn common_affixes_never_split_a_multibyte_character() {
    assert_eq!(common_prefix_byte_len("é", "è"), 0);
    assert_eq!(common_suffix_byte_len("aé", "bé"), "é".len());
    assert_eq!(common_prefix_byte_len("xé", "xé!"), "xé".len());
}

/// Hashed lines carry no identity that survives relocation, so a moved line is a delete plus an
/// insert, as in `diff -u`.
#[test]
fn plain_text_line_diff_never_reports_a_move() {
    let (before, after) = plain_text_line_diff("moved line\na\nb\nc\n", "a\nb\nc\nmoved line\n");
    assert_eq!(
        changed_row_spans(&before),
        vec![(TextOperation::Delete, 0, 1)]
    );
    assert_eq!(
        changed_row_spans(&after),
        vec![(TextOperation::Insert, 3, 4)]
    );
}

/// `myers_lcs` can leave two identical lines unmatched (reordered duplicates); pairing them as a
/// rewrite would claim a match it deliberately did not make.
#[test]
fn shared_affix_declines_two_identical_lines() {
    assert_eq!(shared_affix("same line", "same line"), None);
    assert_eq!(shared_affix("same line", "same lime"), Some((7, 1)));
}

/// A range through the end of a file with no final newline ends one row past the last line, and
/// the trailing trim steps back onto that line rather than dropping the range.
#[test]
fn trailing_trim_keeps_a_range_that_runs_off_a_file_with_no_final_newline() {
    let lines: Vec<&str> = "a\nfoo()".split('\n').collect();
    assert_eq!(
        trim_trailing_whitespace(&lines, &TextRange::new(1, 0, 2, 0)),
        Some(TextRange::new(1, 0, 1, 5))
    );
}

/// Under `FULL`, a wholly new line's insertion grows back over its indentation, even when the
/// matched range before it ends at column 0 of that line.
#[test]
fn full_grows_an_insert_over_the_indentation_of_its_own_line() {
    let source = "fn f() {\n    new();\n}\n";
    let ranges = [
        RangeMatch {
            source: TextRange::new(0, 0, 1, 0),
            destination: TextRange::new(0, 0, 1, 0),
            operation: TextOperation::Identical,
        },
        RangeMatch {
            source: TextRange::new(1, 4, 1, 10),
            destination: TextRange::zero(),
            operation: TextOperation::Insert,
        },
    ];
    let painted = ranges_for_options(&ranges, source, RenderOptions::FULL);
    assert_eq!(painted[1].source, TextRange::new(1, 0, 1, 10));
}

/// Indentation in front of an insertion on a row that also holds matched content belongs to that
/// content, so the insertion does not grow over it (`const ` before a moved declaration).
#[test]
fn full_does_not_grow_an_insert_over_indentation_a_matched_range_shares() {
    let source = "    const int x\n";
    let ranges = [
        RangeMatch {
            source: TextRange::new(0, 4, 0, 10),
            destination: TextRange::zero(),
            operation: TextOperation::Insert,
        },
        RangeMatch {
            source: TextRange::new(0, 10, 0, 15),
            destination: TextRange::new(0, 4, 0, 9),
            operation: TextOperation::Move,
        },
    ];
    let painted = ranges_for_options(&ranges, source, RenderOptions::FULL);
    assert_eq!(painted[0].source, TextRange::new(0, 4, 0, 9));
}

#[test]
fn bracket_pair_partners_skips_a_mismatched_bracket() {
    assert!(bracket_pair_partners("(]").is_empty());
    assert_eq!(bracket_pair_partners("[()]").get(&0), Some(&3));
}

#[test]
fn toggling_an_out_of_range_option_is_a_no_op() {
    let mut options = RenderOptions::FULL;
    options.toggle(options.options().len());
    assert_eq!(options, RenderOptions::FULL);
}
