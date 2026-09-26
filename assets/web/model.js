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

// The viewer's logic, ported from the TUI: what `tui::widgets::code_viewer` (range lookup,
// search navigation, overlay painting order) and `tui::components::diff_viewer` (the
// merged change walk, cross-panel sync, layout mode) do between keystrokes. No DOM and no
// network here - `app.js` owns both - so `node assets/web/model.test.js` covers this file the way
// assets/mapping_site/ is covered, with no framework and no build step.
//
// Every column is a UTF-16 code unit: the server converts from bytes once (src/web/payload.rs),
// so the model indexes the strings it holds directly. Ranges are `[startRow, startCol, endRow,
// endCol]`, half-open, and a range match is `{op, source, destination}` with `op` one of
// insert/delete/update/move/identical/unset.
//
// Text keeps its tabs, which the page draws to the next multiple of the server's `tab_width`
// (CSS `tab-size`). What is measured on screen - horizontal scroll, the viewport width, the sticky
// column, a click's cell - is in display columns, converted from and to UTF-16 columns only by
// `displayColumn`/`columnAtDisplay`: the split `tui::display_columns` makes on the Rust side.
"use strict";

const CodeDiffModel = (() => {
  // src/tui/app.rs FOOTER_HINTS, verbatim - pinned by a Rust test so the two cannot drift.
  const FOOTER_HINTS =
    "?:help  o:open  G:git  r:reload  n/p:next/prev  /:search  M:options  Tab:switch  q:quit";

  const UNCHANGED = new Set(["identical", "unset"]);

  function isEmptyRange(range) {
    return range[0] === range[2] && range[1] === range[3];
  }

  function comparePositions(a, b) {
    return a[0] - b[0] || a[1] - b[1];
  }

  // `build_range_order`: indices sorted by source start, then end, so a zero-width placeholder
  // sharing a start with a real range sorts before it and never wins a lookup.
  function buildRangeOrder(ranges) {
    return ranges
      .map((_, index) => index)
      .sort((a, b) => {
        const x = ranges[a].source;
        const y = ranges[b].source;
        return x[0] - y[0] || x[1] - y[1] || x[2] - y[2] || x[3] - y[3];
      });
  }

  // `range_at`: the last range starting at or before the cursor, if it still covers it.
  function rangeAt(ranges, order, row, col) {
    let lo = 0;
    let hi = order.length;
    while (lo < hi) {
      const mid = (lo + hi) >> 1;
      const s = ranges[order[mid]].source;
      if (s[0] < row || (s[0] === row && s[1] <= col)) lo = mid + 1;
      else hi = mid;
    }
    if (lo === 0) return null;
    const candidate = order[lo - 1];
    const s = ranges[candidate].source;
    return row < s[2] || (row === s[2] && col < s[3]) ? candidate : null;
  }

  function nextPosition(positions, cursor, forward) {
    if (positions.length === 0) return null;
    if (forward) {
      return positions.find((pos) => comparePositions(pos, cursor) > 0) || positions[0];
    }
    for (let i = positions.length - 1; i >= 0; i--) {
      if (comparePositions(positions[i], cursor) < 0) return positions[i];
    }
    return positions[positions.length - 1];
  }

  function countAndIndex(positions, cursor) {
    if (positions.length === 0) return null;
    const passed = positions.filter((pos) => comparePositions(pos, cursor) <= 0).length;
    return [Math.max(passed, 1), positions.length];
  }

  // `TextRange::columns_on_row`.
  function columnsOnRow(range, row, rowLen) {
    if (row < range[0] || row > range[2]) return null;
    const start = row === range[0] ? range[1] : 0;
    const end = row === range[2] ? range[3] : rowLen;
    return start >= end ? null : [start, end];
  }

  // `trailing_whitespace_trimmed_len`: nothing is ever painted on a row's trailing whitespace.
  function trimmedRowLen(line) {
    return line.replace(/\s+$/u, "").length;
  }

  // Code-point iteration with UTF-16 offsets, so a surrogate pair is one character everywhere a
  // character is counted (`chars()` on the Rust side).
  function codePoints(line) {
    const out = [];
    let offset = 0;
    for (const ch of line) {
      out.push({ ch, start: offset, end: offset + ch.length });
      offset += ch.length;
    }
    return out;
  }

  function isWhitespace(ch) {
    return /^\s$/u.test(ch);
  }

  // `non_whitespace_bounds`: `[first, oneAfterLast]` of the real content, or null.
  function nonWhitespaceBounds(line) {
    let first = null;
    let last = null;
    for (const cp of codePoints(line)) {
      if (!isWhitespace(cp.ch)) {
        if (first === null) first = cp.start;
        last = cp.end;
      }
    }
    return first === null ? null : [first, last];
  }

  function clamp(value, low, high) {
    return Math.min(Math.max(value, low), high);
  }

  // `clamp_to_non_whitespace`: a sticky column never lands in indentation or past the content.
  function clampToNonWhitespace(col, line) {
    const bounds = nonWhitespaceBounds(line);
    return bounds ? clamp(col, bounds[0], bounds[1]) : Math.min(col, line.length);
  }

  // `display_columns::char_display_width`: a tab reaches the next tab stop; every other character
  // is one column (a double-width character is one here, where the terminal draws two).
  function charDisplayWidth(ch, column, tabWidth) {
    return ch === "\t" ? tabWidth - (column % tabWidth) : 1;
  }

  // `display_columns::display_column`: the display column UTF-16 column `col` of `line` is drawn
  // at.
  function displayColumn(line, col, tabWidth) {
    let column = 0;
    for (const cp of codePoints(line)) {
      if (cp.end > col) break;
      column += charDisplayWidth(cp.ch, column, tabWidth);
    }
    return column;
  }

  // `display_columns::byte_column_at_display`: the UTF-16 column of the character drawn over
  // display column `target`, or the line's length past its end.
  function columnAtDisplay(line, target, tabWidth) {
    let column = 0;
    for (const cp of codePoints(line)) {
      const width = charDisplayWidth(cp.ch, column, tabWidth);
      if (target < column + width) return cp.start;
      column += width;
    }
    return line.length;
  }

  // One character left/right of `col` in UTF-16 units - two for a surrogate pair.
  function stepLeft(line, col) {
    if (col <= 0) return 0;
    const code = line.charCodeAt(col - 1);
    return col >= 2 && code >= 0xdc00 && code <= 0xdfff ? col - 2 : col - 1;
  }

  function stepRight(line, col) {
    if (col >= line.length) return line.length;
    const code = line.charCodeAt(col);
    return col + 1 < line.length && code >= 0xd800 && code <= 0xdbff ? col + 2 : col + 1;
  }

  // `CodeViewerWidget::find_matches`: smart-case (any capital makes the query exact), one match
  // per starting character including overlapping ones, in document order.
  function findMatches(lines, query) {
    if (!query) return [];
    const caseSensitive = /\p{Lu}/u.test(query);
    const wanted = caseSensitive
      ? Array.from(query)
      : Array.from(query).flatMap((ch) => Array.from(ch.toLowerCase()));
    const matches = [];
    lines.forEach((line, row) => {
      const chars = codePoints(line);
      if (chars.length < wanted.length) return;
      for (let start = 0; start + wanted.length <= chars.length; start++) {
        let ok = true;
        for (let offset = 0; offset < wanted.length; offset++) {
          const candidate = chars[start + offset].ch;
          if (caseSensitive) {
            if (candidate !== wanted[offset]) {
              ok = false;
              break;
            }
          } else {
            const lowered = Array.from(candidate.toLowerCase());
            if (lowered.length !== 1 || lowered[0] !== wanted[offset]) {
              ok = false;
              break;
            }
          }
        }
        if (ok) {
          matches.push([row, chars[start].start, row, chars[start + wanted.length - 1].end]);
        }
      }
    });
    return matches;
  }

  function bandPriority(op) {
    switch (op) {
      case "delete":
        return 4;
      case "insert":
        return 3;
      case "update":
        return 2;
      case "move":
        return 1;
      default:
        return 0;
    }
  }

  // `CodeViewer::change_bands`: the minimap strip, one op (highest priority) per band.
  function changeBands(ranges, total, bands) {
    const out = new Array(bands).fill(null);
    if (bands === 0 || total === 0) return out;
    for (const rm of ranges) {
      if (UNCHANGED.has(rm.op) || isEmptyRange(rm.source)) continue;
      const s = rm.source;
      const lastRow = Math.min(s[3] === 0 ? Math.max(s[2] - 1, 0) : s[2], total - 1);
      for (let row = s[0]; row <= lastRow; row++) {
        const band = Math.min(Math.floor((row * bands) / total), bands - 1);
        if (out[band] === null || bandPriority(rm.op) > bandPriority(out[band])) {
          out[band] = rm.op;
        }
      }
    }
    return out;
  }

  // `DiffViewer::change_stops`: every change once, in before-file order; a paired change stops
  // on the before side, an insertion on the after side keyed by where it belongs in the before
  // file. Panels are 0 (before) and 1 (after).
  function changeStops(beforeRanges, afterRanges) {
    const interesting = (rm) => !UNCHANGED.has(rm.op) && !isEmptyRange(rm.source);
    const stops = [];
    for (const rm of beforeRanges) {
      if (!interesting(rm)) continue;
      const at = [rm.source[0], rm.source[1]];
      stops.push({ key: at, panel: 0, at });
    }
    for (const rm of afterRanges) {
      if (!interesting(rm) || !isEmptyRange(rm.destination)) continue;
      stops.push({
        key: [rm.destination[0], rm.destination[1]],
        panel: 1,
        at: [rm.source[0], rm.source[1]],
      });
    }
    stops.sort((a, b) => comparePositions(a.key, b.key) || a.panel - b.panel);
    const out = [];
    for (const stop of stops) {
      const last = out[out.length - 1];
      if (
        last &&
        comparePositions(last.key, stop.key) === 0 &&
        last.panel === stop.panel &&
        comparePositions(last.at, stop.at) === 0
      ) {
        continue;
      }
      out.push(stop);
    }
    return out.map(({ panel, at }) => ({ panel, at }));
  }

  function compareStops(a, b) {
    return a.panel - b.panel || comparePositions(a.at, b.at);
  }

  // `OverlayPalette::background_for`.
  function backgroundFor(op, palette) {
    switch (op) {
      case "insert":
        return palette.insert_bg;
      case "delete":
        return palette.delete_bg;
      case "move":
        return palette.move_bg;
      case "update":
        return palette.update_bg;
      default:
        return null;
    }
  }

  // Splits `text` into runs of one style. `spans` are the syntax colours (`[start, end, fg]`),
  // `paints` the overlay in application order (`{start, end, bg, fg, cursor?}`) - a later paint
  // wins on the columns it covers, which is `paint_columns` applied in `overlay_row`'s order.
  function rowSegments(text, spans, paints) {
    const edges = new Set([0, text.length]);
    for (const span of spans) {
      edges.add(span[0]);
      edges.add(span[1]);
    }
    for (const paint of paints) {
      edges.add(paint.start);
      edges.add(paint.end);
    }
    const sorted = [...edges].filter((e) => e >= 0 && e <= text.length).sort((a, b) => a - b);
    const segments = [];
    for (let i = 0; i + 1 < sorted.length; i++) {
      const start = sorted[i];
      const end = sorted[i + 1];
      if (start === end) continue;
      let fg = null;
      let bg = null;
      let cursor = false;
      for (const span of spans) {
        if (span[0] <= start && end <= span[1]) {
          fg = span[2];
          break;
        }
      }
      for (const paint of paints) {
        if (paint.start <= start && end <= paint.end) {
          if (paint.cursor) {
            cursor = true;
          } else {
            bg = paint.bg;
            fg = paint.fg;
          }
        }
      }
      const last = segments[segments.length - 1];
      if (last && last.fg === fg && last.bg === bg && last.cursor === cursor && !cursor) {
        last.end = end;
      } else {
        segments.push({ start, end, fg, bg, cursor });
      }
    }
    return segments;
  }

  // `format_change_counts`: `+12 -4 ~2 M1`, zero categories omitted.
  function formatChangeCounts(counts) {
    const parts = [];
    if (counts.insertions > 0) parts.push(`+${counts.insertions}`);
    if (counts.deletions > 0) parts.push(`-${counts.deletions}`);
    if (counts.updates > 0) parts.push(`~${counts.updates}`);
    if (counts.moves > 0) parts.push(`M${counts.moves}`);
    return parts.join(" ");
  }

  function sameOptions(a, b) {
    return Object.keys(b).every((key) => a[key] === b[key]);
  }

  // `render_options_badge`: nothing for FULL, `[minimal]`, or the names of what differs from
  // FULL - not of what is off, since FULL itself leaves whole-pair updates off.
  function renderOptionsBadge(options, rows, presets) {
    if (sameOptions(options, presets.full)) return "";
    if (sameOptions(options, presets.minimal)) return "[minimal]";
    const differing = (state) =>
      rows
        .filter((row) => options[row.key] !== presets.full[row.key] && options[row.key] === state)
        .map((row) => row.label);
    const [off, on] = [differing(false), differing(true)];
    const parts = [];
    if (off.length > 0) parts.push(`${off.join(", ")} off`);
    if (on.length > 0) parts.push(`${on.join(", ")} on`);
    return `[${parts.join("; ")}]`;
  }

  // `DiffViewer::update_display_mode`.
  function displayMode(layoutOverride, widthCells, threshold) {
    if (layoutOverride === "Dual") return "dual";
    if (layoutOverride === "Single") return "single";
    return widthCells < threshold ? "single" : "dual";
  }

  const LAYOUT_CYCLE = { Auto: "Dual", Dual: "Single", Single: "Auto" };
  const LAYOUT_LABEL = { Auto: "auto", Dual: "dual", Single: "single" };

  // `ChangeSet::label`: `working tree`, `staged`, or the abbreviated hash.
  function changeSetLabel(set) {
    if (set.kind === "working_tree") return "working tree";
    if (set.kind === "staged") return "staged";
    return String(set.hash).slice(0, 7);
  }

  // `ChangedFile::label` / `Commit::label`.
  function fileLabel(file) {
    const letters = {
      added: "A",
      modified: "M",
      deleted: "D",
      renamed: "R",
      copied: "C",
      type_changed: "T",
      unmerged: "U",
      untracked: "?",
    };
    const letter = letters[file.status] || "X";
    return file.old_path ? `${letter} ${file.old_path} -> ${file.path}` : `${letter} ${file.path}`;
  }

  function commitLabel(commit) {
    return `${commit.short} ${commit.date} ${commit.subject} (${commit.author})`;
  }

  // `ReviewDialog::rows`: the three sections as one list, commits folding open to their files.
  // `expanded` is one boolean per commit. Only `file` and `commit` rows are selectable.
  function reviewRows(review, expanded) {
    const rows = [];
    const fileRows = (set, files) => {
      files.forEach((file, index) => {
        rows.push({ kind: "file", label: `    ${fileLabel(file)}`, target: { set, file }, index });
      });
    };
    rows.push({ kind: "header", label: `Working tree (${review.working_tree.length})` });
    if (review.working_tree.length === 0) rows.push({ kind: "note", label: "  (clean)" });
    fileRows({ kind: "working_tree" }, review.working_tree);
    rows.push({ kind: "header", label: `Staged (${review.staged.length})` });
    if (review.staged.length === 0) rows.push({ kind: "note", label: "  (nothing staged)" });
    fileRows({ kind: "staged" }, review.staged);
    rows.push({ kind: "header", label: `Recent commits (${review.commits.length})` });
    if (review.commits.length === 0) rows.push({ kind: "note", label: "  (no commits yet)" });
    review.commits.forEach((commit, index) => {
      const marker = expanded[index] ? "\u25be" : "\u25b8";
      rows.push({ kind: "commit", label: `  ${marker} ${commitLabel(commit)}`, commit: index });
      if (expanded[index]) fileRows({ kind: "commit", hash: commit.hash }, commit.files);
    });
    return rows;
  }

  function reviewSelectable(row) {
    return row.kind === "file" || row.kind === "commit";
  }

  // `ReviewDialog::move_selection`: the next selectable row in `direction`, or `selected` itself
  // at the ends.
  function nextReviewSelection(rows, selected, direction) {
    let index = selected;
    for (;;) {
      index += direction < 0 ? -1 : 1;
      if (index < 0 || index >= rows.length) return selected;
      if (reviewSelectable(rows[index])) return index;
    }
  }

  // The files `]`/`[` step through for a target's set.
  function reviewFilesOf(review, set) {
    if (set.kind === "working_tree") return review.working_tree;
    if (set.kind === "staged") return review.staged;
    const commit = review.commits.find((c) => c.hash === set.hash);
    return commit ? commit.files : [];
  }

  // `App::draw_footer`'s left half.
  function footerLeft(info) {
    const parts = [];
    if (info.cursor) parts.push(`Ln ${info.cursor[0] + 1}, Col ${info.cursor[1] + 1}`);
    if (info.counts) {
      const text = formatChangeCounts(info.counts);
      if (text) parts.push(text);
    }
    if (info.searchProgress) parts.push(`match ${info.searchProgress[0]}/${info.searchProgress[1]}`);
    else if (info.changeProgress) parts.push(`change ${info.changeProgress[0]}/${info.changeProgress[1]}`);
    if (info.review) {
      parts.push(`file ${info.review.index + 1}/${info.review.files.length} (${changeSetLabel(info.review.set)})`);
    }
    if (info.plainText) parts.push("[plain text]");
    if (info.layout && info.layout !== "Auto") parts.push(`[layout: ${LAYOUT_LABEL[info.layout]}]`);
    if (info.options && info.rows && info.presets) {
      const badge = renderOptionsBadge(info.options, info.rows, info.presets);
      if (badge) parts.push(badge);
    }
    return parts.join("   ");
  }

  // One panel: `CodeViewer` + `CodeViewerState`.
  class PanelModel {
    constructor() {
      this.path = null;
      this.name = null;
      this.language = "Plain Text";
      this.lines = [];
      this.spans = [];
      this.ranges = [];
      this.order = [];
      this.cursorRow = 0;
      this.cursorCol = 0;
      this.desiredCol = null;
      this.scroll = 0;
      this.scrollCol = 0;
      this.viewportHeight = 0;
      this.viewportWidth = 0;
      this.highlightDestination = null;
      this.searchMatches = [];
      this.focused = false;
      // Until `DiffModel.setTabWidth` hands over the server's `tab_width`, a tab is one column.
      this.tabWidth = 1;
    }

    hasFile() {
      return this.path !== null;
    }

    lineCount() {
      return this.lines.length;
    }

    lineText(row) {
      return this.lines[row] || "";
    }

    lineLen(row) {
      return this.lineText(row).length;
    }

    // `CodeViewer::cursor_display_col`.
    cursorDisplayCol() {
      return displayColumn(this.lineText(this.cursorRow), this.cursorCol, this.tabWidth);
    }

    // `load_contents` + `set_ranges`: cursor on the first navigable range, scrolled into view.
    load(side) {
      this.path = side.path;
      this.name = side.name;
      this.language = side.language;
      this.lines = side.lines;
      this.spans = side.spans;
      this.scroll = 0;
      this.scrollCol = 0;
      this.desiredCol = null;
      this.loadRanges(side.ranges);
      this.scrollToCursor();
    }

    loadRanges(ranges) {
      this.ranges = ranges;
      this.order = buildRangeOrder(ranges);
      const first = this.order.map((i) => ranges[i]).find((rm) => !isEmptyRange(rm.source));
      this.cursorRow = first ? first.source[0] : 0;
      this.cursorCol = first ? first.source[1] : 0;
      this.highlightDestination = null;
      this.searchMatches = [];
    }

    // `replace_ranges`: new ranges, same cursor (clamped) and same search.
    replaceRanges(ranges) {
      this.ranges = ranges;
      this.order = buildRangeOrder(ranges);
      this.cursorRow = Math.min(this.cursorRow, Math.max(this.lineCount() - 1, 0));
      this.highlightDestination = null;
      this.desiredCol = null;
    }

    scrollUp() {
      if (this.scroll > 0) this.scroll -= 1;
    }

    scrollDown() {
      if (this.scroll < Math.max(this.lineCount() - 1, 0)) this.scroll += 1;
    }

    scrollTo(line) {
      this.scroll = Math.min(line, Math.max(this.lineCount() - 1, 0));
    }

    scrollToShowRow(row) {
      row = Math.min(row, Math.max(this.lineCount() - 1, 0));
      if (row < this.scroll) this.scroll = row;
      else if (this.viewportHeight > 0 && row >= this.scroll + this.viewportHeight) {
        this.scroll = row - (this.viewportHeight - 1);
      }
    }

    scrollToCenterRow(row) {
      const total = this.lineCount();
      if (total === 0 || this.viewportHeight === 0) return;
      row = Math.min(row, total - 1);
      const half = Math.floor(this.viewportHeight / 2);
      const maxScroll = Math.max(total - this.viewportHeight, 0);
      this.scroll = Math.min(Math.max(row - half, 0), maxScroll);
    }

    scrollToShowCol(col) {
      const width = this.viewportWidth;
      if (width === 0) return;
      if (col < this.scrollCol) this.scrollCol = col;
      else if (col >= this.scrollCol + width) this.scrollCol = col + 1 - width;
    }

    scrollToCursor() {
      this.scrollToShowRow(this.cursorRow);
      this.scrollToShowCol(this.cursorDisplayCol());
    }

    clampAndSetCursor(row, col) {
      const total = this.lineCount();
      if (total === 0) return false;
      const clampedRow = Math.min(row, total - 1);
      this.cursorRow = clampedRow;
      this.cursorCol = Math.min(col, this.lineLen(clampedRow));
      this.desiredCol = null;
      return true;
    }

    setCursorPosition(row, col) {
      if (this.clampAndSetCursor(row, col)) this.scrollToCursor();
    }

    // `set_cursor_at_display_col`: the character drawn at display column `displayCol`.
    setCursorAtDisplayCol(row, displayCol) {
      row = Math.min(row, Math.max(this.lineCount() - 1, 0));
      this.setCursorPosition(row, columnAtDisplay(this.lineText(row), displayCol, this.tabWidth));
    }

    // The sticky column is a display column, as in `move_cursor_vertical`.
    moveVertical(direction) {
      const total = this.lineCount();
      if (total === 0 || direction === 0) return;
      if (this.desiredCol === null) this.desiredCol = this.cursorDisplayCol();
      const target = this.desiredCol;
      const row = clamp(this.cursorRow + Math.sign(direction), 0, total - 1);
      const line = this.lineText(row);
      this.cursorRow = row;
      this.cursorCol = clampToNonWhitespace(columnAtDisplay(line, target, this.tabWidth), line);
      this.scrollToCursor();
    }

    moveHorizontal(direction) {
      if (direction === 0) return;
      this.desiredCol = null;
      const line = this.lineText(this.cursorRow);
      if (direction < 0) {
        if (this.cursorCol > 0) this.cursorCol = stepLeft(line, this.cursorCol);
        else if (this.cursorRow > 0) {
          this.cursorRow -= 1;
          this.cursorCol = this.lineLen(this.cursorRow);
        }
      } else if (this.cursorCol < line.length) {
        this.cursorCol = stepRight(line, this.cursorCol);
      } else if (this.cursorRow + 1 < this.lineCount()) {
        this.cursorRow += 1;
        this.cursorCol = 0;
      }
      this.scrollToCursor();
    }

    cursor() {
      return [this.cursorRow, this.cursorCol];
    }

    searchPositions() {
      return this.searchMatches.map((m) => [m[0], m[1]]);
    }

    search(query) {
      this.searchMatches = findMatches(this.lines, query);
      const positions = this.searchPositions();
      const nearest =
        positions.find((pos) => comparePositions(pos, this.cursor()) >= 0) || positions[0] || null;
      if (nearest) this.setCursorPosition(nearest[0], nearest[1]);
    }

    previewSearch(query) {
      this.searchMatches = findMatches(this.lines, query);
      return this.searchMatches.length;
    }

    jumpToSearchMatch(forward) {
      const next = nextPosition(this.searchPositions(), this.cursor(), forward);
      if (next) this.setCursorPosition(next[0], next[1]);
    }

    searchMatchCountAndIndex() {
      return countAndIndex(this.searchPositions(), this.cursor());
    }

    rangeAtCursor() {
      return rangeAt(this.ranges, this.order, this.cursorRow, this.cursorCol);
    }

    cursorDestination() {
      const index = this.rangeAtCursor();
      return index === null ? null : this.ranges[index].destination;
    }

    cursorDestinationForHighlight() {
      const index = this.rangeAtCursor();
      if (index === null || this.ranges[index].op === "identical") return null;
      return this.ranges[index].destination;
    }

    changeBands(bands) {
      return changeBands(this.ranges, this.lineCount(), bands);
    }

    // `overlay_row`'s paints for one row, in the order they are applied.
    rowPaints(row, palette, nodeHighlight) {
      const line = this.lineText(row);
      const rowLen = trimmedRowLen(line);
      const paints = [];
      const cursorRange = this.rangeAtCursor();
      this.ranges.forEach((rm, index) => {
        const cols = columnsOnRow(rm.source, row, rowLen);
        if (!cols) return;
        const bg = backgroundFor(rm.op, palette);
        if (bg) paints.push({ start: cols[0], end: cols[1], bg, fg: palette.overlay_fg });
        if (nodeHighlight && this.focused && cursorRange === index && rm.op !== "identical") {
          paints.push({
            start: cols[0],
            end: cols[1],
            bg: palette.cross_highlight_bg,
            fg: palette.overlay_fg,
          });
        }
      });
      for (const match of this.searchMatches) {
        const cols = columnsOnRow(match, row, rowLen);
        if (cols) paints.push({ start: cols[0], end: cols[1], bg: palette.search_bg, fg: palette.overlay_fg });
      }
      if (nodeHighlight && !this.focused && this.highlightDestination) {
        const cols = columnsOnRow(this.highlightDestination, row, rowLen);
        if (cols) {
          paints.push({
            start: cols[0],
            end: cols[1],
            bg: palette.cross_highlight_bg,
            fg: palette.overlay_fg,
          });
        }
      }
      return paints;
    }
  }

  // Both panels: `DiffViewer`.
  class DiffModel {
    constructor() {
      this.panels = [new PanelModel(), new PanelModel()];
      this.activePanel = 0;
      this.layoutOverride = "Auto";
      this.displayMode = "dual";
      this.syncFocus();
    }

    focused() {
      return this.panels[this.activePanel];
    }

    setTabWidth(tabWidth) {
      this.panels.forEach((panel) => (panel.tabWidth = tabWidth));
    }

    other() {
      return this.panels[1 - this.activePanel];
    }

    syncFocus() {
      this.panels[0].focused = this.activePanel === 0;
      this.panels[1].focused = this.activePanel === 1;
    }

    toggleActivePanel() {
      this.activePanel = 1 - this.activePanel;
      this.syncFocus();
    }

    updateDisplayMode(widthCells, threshold) {
      this.displayMode = displayMode(this.layoutOverride, widthCells, threshold);
    }

    cycleLayoutOverride() {
      this.layoutOverride = LAYOUT_CYCLE[this.layoutOverride];
      return this.layoutOverride;
    }

    loadDiff(payload) {
      this.panels[0].load(payload.before);
      this.panels[1].load(payload.after);
      this.activePanel = 0;
      this.syncFocus();
      this.syncCrossHighlight();
    }

    // `set_render_options`' effect on the panels.
    replaceRanges(beforeRanges, afterRanges) {
      this.panels[0].replaceRanges(beforeRanges);
      this.panels[1].replaceRanges(afterRanges);
      this.syncCrossHighlight();
    }

    syncCrossHighlight() {
      const destination = this.focused().cursorDestination();
      this.other().highlightDestination = this.focused().cursorDestinationForHighlight();
      if (destination) this.other().setCursorPosition(destination[0], destination[1]);
    }

    syncScroll() {
      const destination = this.focused().cursorDestination();
      if (destination) this.other().scrollToShowRow(destination[0]);
    }

    syncScrollCentered() {
      const destination = this.focused().cursorDestination();
      if (destination) this.other().scrollToCenterRow(destination[0]);
    }

    moveCursorVertical(direction) {
      this.focused().moveVertical(direction);
      this.syncCrossHighlight();
      this.syncScroll();
    }

    moveCursorHorizontal(direction) {
      this.focused().moveHorizontal(direction);
      this.syncCrossHighlight();
      this.syncScroll();
    }

    focusedCursorPosition() {
      return this.focused().hasFile() ? this.focused().cursor() : null;
    }

    changeStops() {
      return changeStops(this.panels[0].ranges, this.panels[1].ranges);
    }

    currentStopIndex(stops) {
      const cursor = this.focusedCursorPosition();
      if (!cursor) return null;
      const index = stops.findIndex(
        (stop) => stop.panel === this.activePanel && comparePositions(stop.at, cursor) === 0
      );
      return index < 0 ? null : index;
    }

    jumpToChange(forward) {
      const stops = this.changeStops();
      const n = stops.length;
      if (n === 0) return false;
      const current = this.currentStopIndex(stops);
      let next;
      if (current !== null) {
        next = forward ? (current + 1) % n : (current + n - 1) % n;
      } else {
        const after = this.stopsBeforeCursor(stops);
        next = forward ? after % n : (after + n - 1) % n;
      }
      const { panel, at } = stops[next];
      if (this.activePanel !== panel) this.toggleActivePanel();
      this.focused().setCursorPosition(at[0], at[1]);
      this.focused().scrollToCenterRow(at[0]);
      this.syncCrossHighlight();
      this.syncScrollCentered();
      return true;
    }

    // `DiffViewer::stops_before_cursor`.
    stopsBeforeCursor(stops) {
      const here = { panel: this.activePanel, at: this.focusedCursorPosition() || [0, 0] };
      const after = stops.findIndex((stop) => compareStops(stop, here) > 0);
      return after < 0 ? stops.length : after;
    }

    // `DiffViewer::merged_change_count_and_index`: counted in the order `n` walks.
    mergedChangeCountAndIndex() {
      const stops = this.changeStops();
      if (stops.length === 0) return null;
      if (!this.focusedCursorPosition()) return null;
      const current = this.currentStopIndex(stops);
      const index = current !== null ? current + 1 : this.stopsBeforeCursor(stops);
      return [Math.max(index, 1), stops.length];
    }

    moveCursorHalfPage(direction) {
      const half = Math.max(Math.floor(this.focused().viewportHeight / 2), 1);
      for (let i = 0; i < half; i++) this.focused().moveVertical(direction);
      this.syncCrossHighlight();
      this.syncScroll();
    }

    eachScrolling(fn) {
      if (this.displayMode === "dual") this.panels.forEach(fn);
      else fn(this.focused());
    }

    scrollView(direction) {
      this.eachScrolling((panel) => (direction < 0 ? panel.scrollUp() : panel.scrollDown()));
    }

    pageScroll(direction) {
      this.eachScrolling((panel) => {
        for (let i = 0; i < panel.viewportHeight; i++) {
          if (direction < 0) panel.scrollUp();
          else panel.scrollDown();
        }
      });
    }

    // `KeyCode::Home`/`End` in `DiffViewer`: the cursor moves too, not just the view, and in
    // dual mode the other panel shows its own top or bottom.
    home() {
      this.jumpToLine(1);
      if (this.displayMode === "dual") this.panels.forEach((panel) => panel.scrollToCenterRow(0));
    }

    end() {
      this.jumpToLine(this.focused().lineCount());
      if (this.displayMode === "dual") this.panels.forEach((panel) => panel.scrollToCenterRow(panel.lineCount()));
    }

    jumpToCounterpart() {
      const destination = this.focused().cursorDestination();
      if (!destination) return;
      this.toggleActivePanel();
      this.focused().setCursorPosition(destination[0], destination[1]);
      this.syncCrossHighlight();
      this.syncScroll();
    }

    jumpToLine(line) {
      const row = Math.max(line - 1, 0);
      this.focused().setCursorPosition(row, 0);
      this.focused().scrollToCenterRow(row);
      this.syncCrossHighlight();
      this.syncScrollCentered();
    }

    restoreCursor(panel, row, col) {
      if (this.activePanel !== panel) this.toggleActivePanel();
      this.focused().setCursorPosition(row, col);
      this.syncCrossHighlight();
      this.syncScroll();
    }

    previewSearch(query) {
      return this.focused().previewSearch(query);
    }

    search(query) {
      this.focused().search(query);
      this.syncCrossHighlight();
      this.syncScroll();
    }

    jumpToSearchMatch(forward) {
      this.focused().jumpToSearchMatch(forward);
      this.syncCrossHighlight();
      this.syncScroll();
    }

    focusedSearchMatchCountAndIndex() {
      return this.focused().searchMatchCountAndIndex();
    }

    // Mouse: focus the clicked panel and put the cursor on the clicked character, `col` a UTF-16
    // column - or, with `displayCol`, the character drawn in that display column.
    clickAt(panel, row, col, displayCol = false) {
      if (this.activePanel !== panel) this.toggleActivePanel();
      if (displayCol) this.focused().setCursorAtDisplayCol(row, col);
      else this.focused().setCursorPosition(row, col);
      this.syncCrossHighlight();
      this.syncScroll();
    }
  }

  return {
    FOOTER_HINTS,
    LAYOUT_CYCLE,
    LAYOUT_LABEL,
    PanelModel,
    DiffModel,
    backgroundFor,
    buildRangeOrder,
    changeBands,
    changeSetLabel,
    commitLabel,
    fileLabel,
    nextReviewSelection,
    reviewFilesOf,
    reviewRows,
    reviewSelectable,
    changeStops,
    clampToNonWhitespace,
    columnAtDisplay,
    columnsOnRow,
    countAndIndex,
    displayColumn,
    displayMode,
    findMatches,
    footerLeft,
    formatChangeCounts,
    isEmptyRange,
    nextPosition,
    nonWhitespaceBounds,
    rangeAt,
    renderOptionsBadge,
    rowSegments,
    stepLeft,
    stepRight,
    trimmedRowLen,
  };
})();

// Exposed for model.test.js (plain Node) - a no-op in the browser, where `module` is undefined
// and the global below is what app.js reads.
if (typeof module !== "undefined") {
  module.exports = CodeDiffModel;
}
