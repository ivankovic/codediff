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

// Plain-Node tests for model.js - no framework, no npm dependency, same convention as
// assets/mapping_site/. Run: `node assets/web/model.test.js` (wired into `make test` and CI).
//
// Each block mirrors a test in src/tui/widgets/code_viewer.rs or
// src/tui/components/diff_viewer.rs, so the page's behaviour is pinned to the terminal's.
"use strict";

const assert = require("assert");
const M = require("./model.js");

function rm(op, source, destination) {
  return { op, source, destination: destination || source };
}

function side(lines, ranges, name) {
  return { path: `/tmp/${name || "x"}`, name: name || "x", language: "Rust", lines, spans: lines.map(() => []), ranges };
}

const PALETTE = {
  insert_bg: "#001100",
  delete_bg: "#110000",
  move_bg: "#111111",
  update_bg: "#111100",
  overlay_fg: "#eeeeee",
  cross_highlight_bg: "#0000ff",
  search_bg: "#ff8800",
  before_title_fg: "#cd0000",
  after_title_fg: "#00cd00",
};

// range_at_finds_covering_range_and_resolves_ties_and_gaps
{
  const ranges = [
    rm("identical", [0, 5, 0, 9]),
    rm("insert", [0, 5, 0, 5]), // zero-width placeholder sharing the start
    rm("delete", [1, 0, 2, 0]),
  ];
  const order = M.buildRangeOrder(ranges);
  assert.strictEqual(M.rangeAt(ranges, order, 0, 5), 0, "the real range wins the tie");
  assert.strictEqual(M.rangeAt(ranges, order, 0, 8), 0);
  assert.strictEqual(M.rangeAt(ranges, order, 0, 9), null, "half-open");
  assert.strictEqual(M.rangeAt(ranges, order, 0, 2), null, "a gap before any range");
  assert.strictEqual(M.rangeAt(ranges, order, 1, 40), 2, "a multi-row range covers its interior");
  assert.strictEqual(M.rangeAt(ranges, order, 2, 0), null);
}

// next_search_match_position wraps; search_match_count_and_index counts at-or-before the cursor
{
  const positions = [
    [2, 0],
    [5, 3],
    [9, 1],
  ];
  assert.deepStrictEqual(M.nextPosition(positions, [5, 3], true), [9, 1]);
  assert.deepStrictEqual(M.nextPosition(positions, [9, 1], true), [2, 0], "wraps forward");
  assert.deepStrictEqual(M.nextPosition(positions, [2, 0], false), [9, 1], "wraps backward");
  assert.deepStrictEqual(M.nextPosition(positions, [6, 0], false), [5, 3]);
  assert.strictEqual(M.nextPosition([], [0, 0], true), null);
  assert.deepStrictEqual(M.countAndIndex(positions, [0, 0]), [1, 3], "before the first still reads 1");
  assert.deepStrictEqual(M.countAndIndex(positions, [5, 3]), [2, 3]);
  assert.deepStrictEqual(M.countAndIndex(positions, [99, 0]), [3, 3]);
  assert.strictEqual(M.countAndIndex([], [0, 0]), null);
}

// columns_on_row and trailing whitespace
{
  assert.deepStrictEqual(M.columnsOnRow([1, 3, 3, 2], 1, 10), [3, 10]);
  assert.deepStrictEqual(M.columnsOnRow([1, 3, 3, 2], 2, 10), [0, 10]);
  assert.deepStrictEqual(M.columnsOnRow([1, 3, 3, 2], 3, 10), [0, 2]);
  assert.strictEqual(M.columnsOnRow([1, 3, 3, 2], 0, 10), null);
  assert.strictEqual(M.columnsOnRow([1, 3, 3, 0], 3, 10), null, "an end at column 0 covers nothing");
  assert.strictEqual(M.trimmedRowLen("abc   "), 3);
  assert.strictEqual(M.trimmedRowLen("  "), 0);
  assert.strictEqual(M.trimmedRowLen("a "), 1, "a non-breaking space is whitespace");
}

// find_matches: smart case, overlapping starts, non-ASCII columns in UTF-16 units
{
  const lines = ["Foo foo FOO", "aaa", "x éfoo 😀foo"];
  assert.deepStrictEqual(M.findMatches(lines, "foo"), [
    [0, 0, 0, 3],
    [0, 4, 0, 7],
    [0, 8, 0, 11],
    [2, 3, 2, 6],
    [2, 9, 2, 12],
  ]);
  assert.deepStrictEqual(M.findMatches(lines, "Foo"), [[0, 0, 0, 3]], "a capital makes it exact");
  assert.deepStrictEqual(M.findMatches(lines, "aa"), [
    [1, 0, 1, 2],
    [1, 1, 1, 3],
  ], "overlapping starts each count, as in the TUI");
  assert.deepStrictEqual(M.findMatches(lines, ""), []);
  assert.deepStrictEqual(M.findMatches(lines, "😀f"), [[2, 7, 2, 10]], "a surrogate pair is two units");
}

// clamp_to_non_whitespace and the sticky column
{
  assert.strictEqual(M.clampToNonWhitespace(1, "    let x"), 4, "pushed out of the indentation");
  assert.strictEqual(M.clampToNonWhitespace(20, "    let x   "), 9, "pulled back from trailing space");
  assert.strictEqual(M.clampToNonWhitespace(6, "    let x"), 6);
  assert.strictEqual(M.clampToNonWhitespace(3, "      "), 3, "all-whitespace: plain clamp");
  assert.strictEqual(M.clampToNonWhitespace(30, "      "), 6);
  assert.deepStrictEqual(M.nonWhitespaceBounds("  é  "), [2, 3]);
  assert.strictEqual(M.nonWhitespaceBounds("   "), null);

  const panel = new M.PanelModel();
  panel.load(side(["let long_line = 1;", "  x", "let another_long = 2;"], []));
  panel.setCursorPosition(0, 15);
  panel.moveVertical(1);
  assert.deepStrictEqual(panel.cursor(), [1, 3], "clamped to the short line's content");
  panel.moveVertical(1);
  assert.deepStrictEqual(panel.cursor(), [2, 15], "the desired column is remembered");
  panel.moveHorizontal(1);
  panel.moveVertical(-1);
  assert.deepStrictEqual(panel.cursor(), [1, 3], "a horizontal move resets the sticky column");
  panel.moveVertical(-1);
  assert.deepStrictEqual(panel.cursor(), [0, 16], "and the new column is the sticky one from then on");
}

// move_cursor_horizontal wraps at row boundaries and steps over surrogate pairs
{
  const panel = new M.PanelModel();
  panel.load(side(["ab", "😀c"], []));
  panel.setCursorPosition(0, 2);
  panel.moveHorizontal(1);
  assert.deepStrictEqual(panel.cursor(), [1, 0], "right at end of line wraps to the next line");
  panel.moveHorizontal(1);
  assert.deepStrictEqual(panel.cursor(), [1, 2], "one step crosses the whole emoji");
  panel.moveHorizontal(-1);
  assert.deepStrictEqual(panel.cursor(), [1, 0]);
  panel.moveHorizontal(-1);
  assert.deepStrictEqual(panel.cursor(), [0, 2], "left at column 0 wraps to the previous line's end");
  panel.setCursorPosition(0, 0);
  panel.moveHorizontal(-1);
  assert.deepStrictEqual(panel.cursor(), [0, 0], "a no-op at the very start");
  panel.setCursorPosition(1, 3);
  panel.moveHorizontal(1);
  assert.deepStrictEqual(panel.cursor(), [1, 3], "a no-op at the very end");
  assert.strictEqual(M.stepLeft("a😀", 3), 1);
  assert.strictEqual(M.stepRight("a😀", 1), 3);
}

// display_columns: tab stops, and the inverse mapping on every covered column
{
  const line = "\té😀x";
  assert.strictEqual(M.displayColumn(line, 1, 4), 4, "after the tab");
  assert.strictEqual(M.displayColumn(line, 2, 4), 5, "after é");
  assert.strictEqual(M.displayColumn(line, 4, 4), 6, "after the emoji, one column here");
  assert.strictEqual(M.displayColumn(line, 3, 4), 5, "inside a surrogate pair rounds down");
  assert.strictEqual(M.displayColumn("ab\tx", 3, 4), 4, "a tab reaches the next stop");
  assert.strictEqual(M.displayColumn("abcd\tx", 5, 4), 8, "at a stop, a tab is a whole stop");
  for (const [display, col] of [[0, 0], [3, 0], [4, 1], [5, 2], [6, 4], [7, 5]]) {
    assert.strictEqual(M.columnAtDisplay(line, display, 4), col, `display column ${display}`);
  }
}

// move_cursor_vertical_keeps_the_display_column_across_tab_and_space_indentation
{
  const panel = new M.PanelModel();
  panel.tabWidth = 4;
  panel.load(side(["\tfoo", "    bar"], []));
  panel.setCursorPosition(0, 2); // the second `o`, drawn in column 5
  panel.moveVertical(1);
  assert.deepStrictEqual(panel.cursor(), [1, 5], "the `a` of `bar`, also column 5");
  panel.moveVertical(-1);
  assert.deepStrictEqual(panel.cursor(), [0, 2]);
}

// horizontal_scroll_follows_the_cursor_in_display_columns,
// set_cursor_at_display_col_lands_on_the_character_drawn_there
{
  const panel = new M.PanelModel();
  panel.tabWidth = 4;
  panel.load(side(["\t\t\t\tx", "\tab"], []));
  panel.viewportWidth = 5;
  panel.setCursorPosition(0, 4); // `x`, drawn in column 16
  assert.strictEqual(panel.scrollCol, 12);
  panel.setCursorPosition(0, 1); // the second tab, from column 4
  assert.strictEqual(panel.scrollCol, 4);

  panel.setCursorAtDisplayCol(1, 2);
  assert.deepStrictEqual(panel.cursor(), [1, 0], "inside the tab");
  panel.setCursorAtDisplayCol(1, 5);
  assert.deepStrictEqual(panel.cursor(), [1, 2], "the `b`");
  panel.setCursorAtDisplayCol(1, 40);
  assert.deepStrictEqual(panel.cursor(), [1, 3], "past the end");
}

// load_ranges_places_cursor_on_first_navigable_position; scrolling follows the cursor
{
  const panel = new M.PanelModel();
  panel.viewportHeight = 5;
  panel.viewportWidth = 10;
  panel.load(side(Array.from({ length: 40 }, (_, i) => `line ${i} with some text`), [
    rm("insert", [20, 0, 20, 0], [0, 0, 0, 0]),
    rm("delete", [30, 5, 30, 9], [7, 0, 7, 0]),
    rm("identical", [35, 0, 35, 4]),
  ]));
  assert.deepStrictEqual(panel.cursor(), [30, 5], "the placeholder is skipped");
  assert.strictEqual(panel.scroll, 26, "scrolled just enough to show the cursor row");
  panel.scrollToCenterRow(30);
  assert.strictEqual(panel.scroll, 28);
  panel.setCursorPosition(2, 15);
  assert.strictEqual(panel.scroll, 2);
  assert.strictEqual(panel.scrollCol, 6, "horizontal scroll follows too");
  panel.scrollToCenterRow(39);
  assert.strictEqual(panel.scroll, 35, "clamped so the last page is full");
}

// change_bands: priority delete > insert > update > move, per band
{
  const ranges = [rm("move", [0, 0, 1, 0]), rm("update", [0, 2, 0, 5]), rm("insert", [9, 0, 10, 0]), rm("delete", [9, 3, 9, 4])];
  assert.deepStrictEqual(M.changeBands(ranges, 10, 5), ["update", null, null, null, "delete"]);
  assert.deepStrictEqual(M.changeBands(ranges, 0, 3), [null, null, null]);
  assert.deepStrictEqual(M.changeBands([], 10, 0), []);
}

// change_stops: paired changes stop on the before side, insertions on the after side, in
// before-file order; the same key stops before before after
{
  const before = [rm("delete", [5, 0, 5, 3], [3, 0, 3, 0]), rm("update", [1, 2, 1, 4], [1, 2, 1, 4]), rm("identical", [0, 0, 0, 9])];
  const after = [
    rm("insert", [3, 0, 3, 8], [5, 0, 5, 0]), // belongs where the deletion was
    rm("update", [1, 2, 1, 4], [1, 2, 1, 4]), // paired: not a stop here
    rm("insert", [0, 0, 0, 0], [0, 0, 0, 0]), // empty placeholder
  ];
  assert.deepStrictEqual(M.changeStops(before, after), [
    { panel: 0, at: [1, 2] },
    { panel: 0, at: [5, 0] },
    { panel: 1, at: [3, 0] },
  ]);
}

function pairModel() {
  const model = new M.DiffModel();
  model.panels.forEach((panel) => {
    panel.viewportHeight = 10;
    panel.viewportWidth = 80;
  });
  const lines = Array.from({ length: 30 }, (_, i) => `line ${i}`);
  model.loadDiff({
    before: side(lines, [rm("delete", [5, 0, 5, 6], [5, 0, 5, 0]), rm("update", [12, 5, 12, 6], [12, 5, 12, 6]), rm("identical", [0, 0, 0, 6])], "before"),
    after: side(lines, [rm("insert", [5, 0, 5, 6], [5, 0, 5, 0]), rm("update", [12, 5, 12, 6], [12, 5, 12, 6]), rm("insert", [20, 0, 20, 7], [19, 0, 19, 0])], "after"),
  });
  return model;
}

// n crosses to the other panel when that is where the next change is, and the counter reports
// the merged total
{
  const model = pairModel();
  assert.strictEqual(model.activePanel, 0);
  assert.deepStrictEqual(model.focusedCursorPosition(), [0, 0], "loaded on the first navigable range");
  assert.deepStrictEqual(model.mergedChangeCountAndIndex(), [1, 4]);
  assert.ok(model.jumpToChange(true));
  assert.deepStrictEqual([model.activePanel, model.focusedCursorPosition()], [0, [5, 0]]);
  model.jumpToChange(true);
  assert.deepStrictEqual([model.activePanel, model.focusedCursorPosition()], [1, [5, 0]], "the replacing insertion, on the after side");
  assert.deepStrictEqual(model.mergedChangeCountAndIndex(), [2, 4]);
  model.jumpToChange(true);
  assert.deepStrictEqual([model.activePanel, model.focusedCursorPosition()], [0, [12, 5]]);
  // Counted in walk order, so it climbs across the panel switch (by (panel, position) it read 2).
  assert.deepStrictEqual(model.mergedChangeCountAndIndex(), [3, 4]);
  assert.strictEqual(model.panels[0].scroll, 7, "the focused panel centres the change");
  model.jumpToChange(true);
  assert.deepStrictEqual([model.activePanel, model.focusedCursorPosition()], [1, [20, 0]]);
  model.jumpToChange(true);
  assert.deepStrictEqual([model.activePanel, model.focusedCursorPosition()], [0, [5, 0]], "wraps");
  model.jumpToChange(false);
  assert.deepStrictEqual([model.activePanel, model.focusedCursorPosition()], [1, [20, 0]], "p wraps the other way");
}

// End and Home move the cursor to the last and first line, not just the view
{
  const model = pairModel();
  model.end();
  assert.deepStrictEqual(model.focusedCursorPosition(), [29, 0]);
  model.home();
  assert.deepStrictEqual(model.focusedCursorPosition(), [0, 0]);
}

// Enter jumps to the counterpart and back; the other panel's cursor follows the match
{
  const model = pairModel();
  model.focused().setCursorPosition(12, 5);
  model.syncCrossHighlight();
  assert.deepStrictEqual(model.panels[1].cursor(), [12, 5], "the inactive side follows the matched node");
  assert.deepStrictEqual(model.panels[1].highlightDestination, [12, 5, 12, 6]);
  model.jumpToCounterpart();
  assert.strictEqual(model.activePanel, 1);
  assert.deepStrictEqual(model.focusedCursorPosition(), [12, 5]);
  model.jumpToCounterpart();
  assert.strictEqual(model.activePanel, 0);
  model.focused().setCursorPosition(0, 1);
  model.syncCrossHighlight();
  assert.strictEqual(model.panels[1].highlightDestination, null, "an identical match is never highlighted");
  assert.deepStrictEqual(model.panels[1].cursor(), [0, 0], "but the cursor still follows");
}

// jumping with no changes anywhere does nothing
{
  const model = new M.DiffModel();
  model.loadDiff({ before: side(["a"], [rm("identical", [0, 0, 0, 1])]), after: side(["a"], [rm("identical", [0, 0, 0, 1])]) });
  assert.strictEqual(model.jumpToChange(true), false);
  assert.strictEqual(model.mergedChangeCountAndIndex(), null);
}

// search: nearest match at or after the cursor, >/< step and wrap, count/index in the footer
{
  const model = pairModel();
  assert.strictEqual(model.previewSearch("line 1"), 11);
  model.focused().setCursorPosition(3, 0);
  model.search("line 2"); // "line 2" itself, then "line 20".."line 29": 11 matches
  assert.deepStrictEqual(model.focusedCursorPosition(), [20, 0], "the nearest match at or after the cursor");
  assert.deepStrictEqual(model.focusedSearchMatchCountAndIndex(), [2, 11]);
  model.jumpToSearchMatch(true);
  assert.deepStrictEqual(model.focusedCursorPosition(), [21, 0]);
  model.jumpToSearchMatch(false);
  model.jumpToSearchMatch(false);
  assert.deepStrictEqual(model.focusedCursorPosition(), [2, 0]);
  model.jumpToSearchMatch(false);
  assert.deepStrictEqual(model.focusedCursorPosition(), [29, 0], "wraps backward from the first");
  model.search("");
  assert.deepStrictEqual(model.focused().searchMatches, [], "an empty query clears the highlights");
  assert.strictEqual(model.focusedSearchMatchCountAndIndex(), null);
}

// jump_to_line, restore_cursor, the layout cycle, Tab, and half-page/page scrolling
{
  const model = pairModel();
  model.jumpToLine(25);
  assert.deepStrictEqual(model.focusedCursorPosition(), [24, 0]);
  assert.strictEqual(model.panels[0].scroll, 19);
  model.jumpToLine(0);
  assert.deepStrictEqual(model.focusedCursorPosition(), [0, 0]);
  model.restoreCursor(1, 99, 99);
  assert.deepStrictEqual([model.activePanel, model.focusedCursorPosition()], [1, [29, 7]], "clamped");
  model.toggleActivePanel();
  assert.strictEqual(model.activePanel, 0);
  assert.ok(model.panels[0].focused && !model.panels[1].focused);
  assert.strictEqual(model.cycleLayoutOverride(), "Dual");
  assert.strictEqual(model.cycleLayoutOverride(), "Single");
  assert.strictEqual(model.cycleLayoutOverride(), "Auto");
  model.focused().setCursorPosition(0, 0);
  model.moveCursorHalfPage(1);
  assert.deepStrictEqual(model.focusedCursorPosition(), [5, 0]);
  model.updateDisplayMode(300, 220);
  assert.strictEqual(model.displayMode, "dual");
  model.panels.forEach((p) => p.scrollTo(0));
  model.pageScroll(1);
  assert.deepStrictEqual(model.panels.map((p) => p.scroll), [10, 10], "both panels page in dual mode");
  model.scrollView(-1);
  assert.deepStrictEqual(model.panels.map((p) => p.scroll), [9, 9]);
  model.end();
  // The last full screen, not the last line alone at the top.
  assert.deepStrictEqual(model.panels.map((p) => p.scroll), [20, 20]);
  model.home();
  assert.deepStrictEqual(model.panels.map((p) => p.scroll), [0, 0]);
  model.layoutOverride = "Single";
  model.updateDisplayMode(300, 220);
  assert.strictEqual(model.displayMode, "single");
  model.pageScroll(1);
  assert.deepStrictEqual(model.panels.map((p) => p.scroll), [10, 0], "only the focused panel in single mode");
}

// display mode follows the override, then the width threshold
{
  assert.strictEqual(M.displayMode("Auto", 219, 220), "single");
  assert.strictEqual(M.displayMode("Auto", 220, 220), "dual");
  assert.strictEqual(M.displayMode("Dual", 10, 220), "dual");
  assert.strictEqual(M.displayMode("Single", 1000, 220), "single");
}

// overlay painting: diff colour, node highlight only on the focused side and never on an
// identical match, search on top, nothing on trailing whitespace, cursor as its own run
{
  const panel = new M.PanelModel();
  panel.load(side(["abcdef   "], [rm("update", [0, 1, 0, 4], [0, 1, 0, 4]), rm("identical", [0, 4, 0, 9])]));
  panel.focused = true;
  panel.setCursorPosition(0, 2);
  let paints = panel.rowPaints(0, PALETTE, false);
  assert.deepStrictEqual(paints, [{ start: 1, end: 4, bg: PALETTE.update_bg, fg: PALETTE.overlay_fg }]);
  paints = panel.rowPaints(0, PALETTE, true);
  assert.strictEqual(paints.length, 2);
  assert.strictEqual(paints[1].bg, PALETTE.cross_highlight_bg, "node highlight on the focused range");
  panel.setCursorPosition(0, 5);
  assert.strictEqual(panel.rowPaints(0, PALETTE, true).length, 1, "an identical range gets no highlight");
  panel.focused = false;
  panel.highlightDestination = [0, 0, 0, 9];
  paints = panel.rowPaints(0, PALETTE, true);
  // The end column of a range's own last row is used as is (`columns_on_row`); only interior
  // rows clip to the trimmed length. Trailing whitespace is kept out of ranges upstream, by
  // `ranges_for_options`, not here.
  assert.deepStrictEqual(paints[paints.length - 1], { start: 0, end: 9, bg: PALETTE.cross_highlight_bg, fg: PALETTE.overlay_fg });
  panel.highlightDestination = [0, 0, 1, 2];
  assert.deepStrictEqual(panel.rowPaints(0, PALETTE, true).pop(), { start: 0, end: 6, bg: PALETTE.cross_highlight_bg, fg: PALETTE.overlay_fg }, "an interior row clips to the trimmed length");
  panel.highlightDestination = [0, 0, 0, 9];
  assert.strictEqual(panel.rowPaints(0, PALETTE, false).length, 1, "off means off");
  panel.searchMatches = [[0, 2, 0, 3]];
  paints = panel.rowPaints(0, PALETTE, false);
  assert.deepStrictEqual(paints[1], { start: 2, end: 3, bg: PALETTE.search_bg, fg: PALETTE.overlay_fg });

  const segments = M.rowSegments("abcdef   ", [[0, 9, "#ff0000"]], [...paints, { start: 4, end: 5, cursor: true }]);
  assert.deepStrictEqual(segments, [
    { start: 0, end: 1, fg: "#ff0000", bg: null, cursor: false },
    { start: 1, end: 2, fg: PALETTE.overlay_fg, bg: PALETTE.update_bg, cursor: false },
    { start: 2, end: 3, fg: PALETTE.overlay_fg, bg: PALETTE.search_bg, cursor: false },
    { start: 3, end: 4, fg: PALETTE.overlay_fg, bg: PALETTE.update_bg, cursor: false },
    { start: 4, end: 5, fg: "#ff0000", bg: null, cursor: true },
    { start: 5, end: 9, fg: "#ff0000", bg: null, cursor: false },
  ]);
  assert.deepStrictEqual(M.rowSegments("", [], []), []);
  assert.deepStrictEqual(M.rowSegments("ab", [], []), [{ start: 0, end: 2, fg: null, bg: null, cursor: false }]);
}

// footer text: change counts, progress, badges
{
  assert.strictEqual(M.formatChangeCounts({ insertions: 12, deletions: 4, updates: 2, moves: 1 }), "+12 -4 ~2 M1");
  assert.strictEqual(M.formatChangeCounts({ insertions: 0, deletions: 0, updates: 0, moves: 0 }), "");
  const rows = [
    { key: "leading_whitespace", label: "Leading whitespace" },
    { key: "structural_punctuation", label: "Structural punctuation (brackets, separators)" },
    { key: "whole_pair_updates", label: "Whole-pair updates" },
    { key: "paint_reindent_only_moves", label: "Paint reindent-only moves" },
    { key: "paint_displaced_moves", label: "Paint displaced moves" },
    { key: "paint_resized_moves", label: "Paint moves the two sides size differently" },
  ];
  const full = { leading_whitespace: true, structural_punctuation: true, whole_pair_updates: false, paint_reindent_only_moves: true, paint_displaced_moves: true, paint_resized_moves: true };
  const minimal = { leading_whitespace: false, structural_punctuation: false, whole_pair_updates: false, paint_reindent_only_moves: false, paint_displaced_moves: false, paint_resized_moves: false };
  const presets = { full, minimal };
  assert.strictEqual(M.renderOptionsBadge(full, rows, presets), "");
  assert.strictEqual(M.renderOptionsBadge(minimal, rows, presets), "[minimal]");
  // "Whole-pair updates" is off in FULL itself, so only a difference from FULL is named.
  assert.strictEqual(
    M.renderOptionsBadge({ ...full, leading_whitespace: false }, rows, presets),
    "[Leading whitespace off]"
  );
  assert.strictEqual(M.renderOptionsBadge({ ...full, whole_pair_updates: true }, rows, presets), "[Whole-pair updates on]");
  assert.strictEqual(
    M.renderOptionsBadge({ ...full, leading_whitespace: false, whole_pair_updates: true }, rows, presets),
    "[Leading whitespace off; Whole-pair updates on]"
  );
  assert.strictEqual(
    M.footerLeft({ cursor: [4, 9], counts: { insertions: 1, deletions: 0, updates: 0, moves: 0 }, changeProgress: [2, 5], plainText: true, layout: "Single", options: minimal, rows, presets }),
    "Ln 5, Col 10   +1   change 2/5   [plain text]   [layout: single]   [minimal]"
  );
  assert.strictEqual(
    M.footerLeft({ cursor: [0, 0], searchProgress: [1, 3], changeProgress: [2, 5], layout: "Auto", options: full, rows, presets }),
    "Ln 1, Col 1   match 1/3",
    "search progress replaces change progress"
  );
  assert.strictEqual(M.footerLeft({}), "");
  assert.ok(M.FOOTER_HINTS.startsWith("?:help"));
}

// replaceRanges keeps the cursor (clamped) and the search, drops the stale counterpart
{
  const model = pairModel();
  model.search("line 2");
  model.focused().setCursorPosition(29, 3);
  model.syncCrossHighlight();
  model.replaceRanges([rm("delete", [1, 0, 1, 2], [1, 0, 1, 0])], []);
  assert.deepStrictEqual(model.focusedCursorPosition(), [29, 3]);
  assert.strictEqual(model.focused().searchMatches.length, 11);
  assert.deepStrictEqual(model.changeStops(), [{ panel: 0, at: [1, 0] }]);
}

// a click focuses the panel and places the cursor, clamped to the line
{
  const model = pairModel();
  model.clickAt(1, 3, 99);
  assert.deepStrictEqual([model.activePanel, model.focusedCursorPosition()], [1, [3, 6]]);
}

// The cursor lands on a change, search hit or counterpart after non-ASCII text or tabs, and the
// range under it is the one it is drawn on - DiffViewer's `*_after_non_ascii_text_or_tabs` tests.
// `oneUpdateOnOneLine` is `one_update_on_one_line`: one update between unchanged text.
function oneUpdateOnOneLine(before, [start, end], after, [afterStart, afterEnd]) {
  const ranges = (s, e, os, oe) => [
    rm("identical", [0, 0, 0, s], [0, 0, 0, os]),
    rm("update", [0, s, 0, e], [0, os, 0, oe]),
    rm("identical", [0, e, 1, 0], [0, oe, 1, 0]),
  ];
  const model = new M.DiffModel();
  model.setTabWidth(4);
  model.panels.forEach((panel) => {
    panel.viewportHeight = 10;
    panel.viewportWidth = 80;
  });
  model.loadDiff({
    before: side([before], ranges(start, end, afterStart, afterEnd), "before"),
    after: side([after], ranges(afterStart, afterEnd, start, end), "after"),
  });
  return model;
}

// n_puts_the_cursor_on_a_change_after_non_ascii_text, n_and_p_..._after_tab_indentation
{
  const model = oneUpdateOnOneLine("let é = old;", [8, 11], "let é = new;", [8, 11]);
  model.jumpToChange(true);
  assert.deepStrictEqual(model.focusedCursorPosition(), [0, 8], "on the `o`");
  assert.deepStrictEqual(model.panels[1].cursor(), [0, 8], "the other panel follows");

  const tabbed = oneUpdateOnOneLine("\t\told();", [2, 5], "\t\tnew();", [2, 5]);
  tabbed.jumpToChange(true);
  assert.deepStrictEqual(tabbed.focusedCursorPosition(), [0, 2]);
  assert.strictEqual(tabbed.focused().cursorDisplayCol(), 8, "drawn after two tab stops");
  tabbed.focused().setCursorPosition(0, 0);
  tabbed.jumpToChange(false);
  assert.deepStrictEqual(tabbed.focusedCursorPosition(), [0, 2]);
}

// search_puts_the_cursor_on_a_match_after_non_ascii_text_or_tabs
{
  for (const [line, col, display] of [
    ["é = world", 4, 4],
    ["漢字 = world", 5, 5],
    ["\t\tworld", 2, 8],
  ]) {
    const model = oneUpdateOnOneLine(line, [0, 0], "x", [0, 0]);
    model.search("world");
    assert.deepStrictEqual(model.focusedCursorPosition(), [0, col], line);
    assert.strictEqual(model.focused().cursorDisplayCol(), display, line);
  }
}

// enter_puts_the_cursor_on_a_counterpart_after_non_ascii_text_or_tabs
{
  for (const [after, range, display] of [
    ["é = new;", [4, 7], 4],
    ["\tnew;", [1, 4], 4],
  ]) {
    const model = oneUpdateOnOneLine("old;", [0, 3], after, range);
    model.focused().setCursorPosition(0, 0);
    model.jumpToCounterpart();
    assert.strictEqual(model.activePanel, 1);
    assert.deepStrictEqual(model.focusedCursorPosition(), [0, range[0]], after);
    assert.strictEqual(model.focused().cursorDisplayCol(), display, after);
  }
}

// the_range_under_a_cursor_moved_past_non_ascii_text_is_the_one_it_is_drawn_on,
// a_click_on_a_tab_indented_row_selects_the_range_drawn_under_it
{
  const model = oneUpdateOnOneLine("é = ab", [4, 6], "x = ab", [4, 6]);
  model.focused().setCursorPosition(0, 0);
  for (let i = 0; i < 4; i++) model.moveCursorHorizontal(1);
  assert.deepStrictEqual(model.focusedCursorPosition(), [0, 4]);
  assert.deepStrictEqual(model.focused().cursorDestination(), [0, 4, 0, 6]);

  const tabbed = oneUpdateOnOneLine("\tx = ab", [5, 7], "x = ab", [4, 6]);
  tabbed.clickAt(0, 0, 9, true); // `    x = ab`: the `b` is in display column 9
  assert.deepStrictEqual(tabbed.focusedCursorPosition(), [0, 6]);
  assert.deepStrictEqual(tabbed.focused().cursorDestination(), [0, 4, 0, 6]);
}

// The git review picker's rows and navigation mirror ReviewDialog's tests
{
  const file = (status, path, old_path) => ({ status, path, ...(old_path ? { old_path } : {}) });
  const review = {
    root: "/repo",
    working_tree: [file("modified", "a.rs")],
    staged: [],
    commits: [
      { hash: "aaaa", short: "aaaa", author: "Ada", date: "2026-09-10", subject: "newest", files: [file("added", "n.rs"), file("deleted", "o.rs")] },
      { hash: "bbbb", short: "bbbb", author: "Bob", date: "2026-09-09", subject: "older", files: [file("renamed", "new.rs", "old.rs")] },
    ],
  };
  const rows = M.reviewRows(review, [true, false]);
  assert.deepStrictEqual(
    rows.map((row) => row.label),
    [
      "Working tree (1)",
      "    M a.rs",
      "Staged (0)",
      "  (nothing staged)",
      "Recent commits (2)",
      "  \u25be aaaa 2026-09-10 newest (Ada)",
      "    A n.rs",
      "    D o.rs",
      "  \u25b8 bbbb 2026-09-09 older (Bob)",
    ]
  );
  assert.deepStrictEqual(rows[7].target, { set: { kind: "commit", hash: "aaaa" }, file: file("deleted", "o.rs") });
  assert.strictEqual(rows[7].index, 1);
  assert.strictEqual(M.nextReviewSelection(rows, 1, -1), 1, "nothing selectable above the first file");
  assert.strictEqual(M.nextReviewSelection(rows, 1, 1), 5, "straight over the empty Staged section");
  assert.strictEqual(M.nextReviewSelection(rows, 8, 1), 8, "stays at the end");
  assert.strictEqual(M.reviewRows(review, [false, true]).length, 8);
  assert.strictEqual(M.reviewRows(review, [false, true])[7].label, "    R old.rs -> new.rs");
  assert.strictEqual(M.reviewFilesOf(review, { kind: "commit", hash: "bbbb" }).length, 1);
  assert.strictEqual(M.reviewFilesOf(review, { kind: "commit", hash: "nope" }).length, 0);
  assert.strictEqual(M.reviewFilesOf(review, { kind: "working_tree" }).length, 1);
  assert.strictEqual(M.changeSetLabel({ kind: "commit", hash: "0123456789" }), "0123456");
  assert.strictEqual(M.changeSetLabel({ kind: "staged" }), "staged");
  const empty = M.reviewRows({ working_tree: [], staged: [], commits: [] }, []);
  assert.deepStrictEqual(empty.map((row) => row.kind), ["header", "note", "header", "note", "header", "note"]);
  assert.strictEqual(M.nextReviewSelection(empty, 0, 1), 0);
  assert.strictEqual(
    M.footerLeft({ review: { set: { kind: "working_tree" }, files: [1, 2], index: 1 } }),
    "file 2/2 (working tree)"
  );
}

console.log("model.test.js: all assertions passed");
