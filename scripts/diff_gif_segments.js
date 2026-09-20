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

// Flattens a baked showcase payload into per-row coloured runs, for scripts/record_diff_gif.py to
// draw.
//
// This exists so the GIF is painted by the viewer's own logic rather than by a second
// implementation of it. `assets/web/model.js` decides which columns a range covers on a row, that
// trailing whitespace is never painted, and which paint wins where they overlap - rules with
// enough corners (`columnsOnRow`, `trimmedRowLen`, the paint order in `rowPaints`) that a Python
// re-implementation would drift from the browser without anyone noticing, and the GIF would then
// advertise a rendering the product does not produce. Node is already a dev dependency here: this
// is the same file `make test-web-js` covers.
//
//   node scripts/diff_gif_segments.js <payload.json> <state.json>
//
// stdout is `{before: {name, language, rows: [[{text, fg, bg}, ...], ...]}, after: {...}}`, where
// `fg`/`bg` are `#rrggbb` or null.

"use strict";

const fs = require("fs");
const path = require("path");

const M = require(path.join(__dirname, "..", "assets", "web", "model.js"));

function sideRows(side, palette) {
  const panel = new M.PanelModel();
  panel.load(side);
  const rows = [];
  for (let row = 0; row < panel.lineCount(); row += 1) {
    // `false` for nodeHighlight: the cross-panel counterpart highlight follows a cursor, and a
    // recording has no cursor to follow. Everything else is the panel as the browser paints it.
    const text = panel.lineText(row);
    const paints = panel.rowPaints(row, palette, false);
    const segments = M.rowSegments(text, panel.spans[row] || [], paints);
    // Sliced here rather than in the renderer: `rowSegments` indexes UTF-16 code units, which is
    // what a JavaScript string slice uses natively and what a Python one does not.
    rows.push(
      segments.map((segment) => ({
        text: text.slice(segment.start, segment.end),
        fg: segment.fg || null,
        bg: segment.bg || null,
      })),
    );
  }
  return { name: side.name, language: side.language, rows };
}

function main() {
  const [payloadPath, statePath] = process.argv.slice(2);
  if (!payloadPath || !statePath) {
    console.error("usage: node scripts/diff_gif_segments.js <payload.json> <state.json>");
    process.exit(2);
  }
  const payload = JSON.parse(fs.readFileSync(payloadPath, "utf8"));
  const state = JSON.parse(fs.readFileSync(statePath, "utf8"));
  // The theme the viewer would open with, so the recording matches what a reader sees there.
  const wanted = state.settings && state.settings.theme;
  const theme = state.themes.find((t) => t.id === wanted) || state.themes[0];
  const palette = theme.palette;
  process.stdout.write(
    JSON.stringify({
      palette,
      before: sideRows(payload.before, palette),
      after: sideRows(payload.after, palette),
    }),
  );
}

main();
