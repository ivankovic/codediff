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

// Run with `node assets/showcase/showcase.test.js` (`make test-showcase-js`). Plain Node, no
// framework, like the mapping site's and the viewer's own tests: the DOM wiring is not exercised,
// the parts that decide what the page shows are.
"use strict";
const assert = require("node:assert/strict");
const S = require("./showcase.js");

const cases = [
  { name: "a", group: "diff_wrong", title: "A", lines: 10, diff_marked: 4, codediff: { updates: 1, moves: 0, insertions: 0, deletions: 0 }, summary: null },
  { name: "b", group: "both_right", title: "B", lines: 3, diff_marked: 1, codediff: { updates: 0, moves: 1, insertions: 2, deletions: 0 }, summary: "Whitespace changes only" },
];

// ----- parseQuery / buildQuery ----------------------------------------------------------------

{
  assert.deepEqual(S.parseQuery("", cases), { name: "a", view: "diff" });
  assert.deepEqual(S.parseQuery("?case=b&view=codediff", cases), { name: "b", view: "codediff" });
  // Unknown case or view: fall back, never leave the page empty.
  assert.deepEqual(S.parseQuery("?case=nope&view=banana", cases), { name: "a", view: "diff" });
  assert.deepEqual(S.parseQuery("?view=codediff", cases), { name: "a", view: "codediff" });
  assert.deepEqual(S.parseQuery("", []), { name: null, view: "diff" });
  assert.equal(S.buildQuery("b", "codediff"), "?case=b&view=codediff");
  const round = S.parseQuery(S.buildQuery("b", "codediff"), cases);
  assert.deepEqual(round, { name: "b", view: "codediff" });
}

// ----- payloadFor -----------------------------------------------------------------------------

const presets = { minimal: { x: false, y: false }, full: { x: true, y: true } };
const payloads = {
  diff: { id: "diff", before: { spans: [1] }, after: { spans: [2] }, syntax: { theme: "t" } },
  codediff: { id: "codediff", before: { spans: [3] }, after: { spans: [4] }, syntax: { theme: "t" } },
  minimal: { id: "minimal" },
  full: { id: "full" },
};

{
  assert.equal(S.payloadFor("diff", payloads, presets, presets.full).id, "diff");
  assert.equal(S.payloadFor("codediff", payloads, presets, null).id, "codediff");
  assert.equal(S.payloadFor("codediff", payloads, presets, { x: false, y: false }).id, "minimal");
  assert.equal(S.payloadFor("codediff", payloads, presets, { x: true, y: true }).id, "full");
  // Anything that is not a preset gets the default: there is nothing to recompute with.
  assert.equal(S.payloadFor("codediff", payloads, presets, { x: true, y: false }).id, "codediff");
}

// ----- routeApi -------------------------------------------------------------------------------

function ctx(view) {
  return { state: { presets, settings: {} }, view, payloads, presets, options: null };
}

{
  const state = S.routeApi("/api/state", {}, ctx("diff"));
  assert.equal(state.status, 200);
  // A pair is always "open", so the viewer starts diffing on load.
  assert.equal(state.json.before, "before");
  assert.equal(state.json.after, "after");
  assert.equal(state.json.presets, presets);

  assert.equal(S.routeApi("/api/diff", {}, ctx("diff")).json.id, "diff");
  assert.equal(S.routeApi("/api/diff", {}, ctx("codediff")).json.id, "codediff");

  // Render options are remembered for the next /api/diff and echoed in the answer.
  const c = ctx("codediff");
  const filtered = S.routeApi("/api/render_options", presets.full, c);
  assert.equal(filtered.json.id, "full");
  assert.deepEqual(filtered.json.render_options, presets.full);
  assert.equal(S.routeApi("/api/diff", {}, c).json.id, "full");

  const highlight = S.routeApi("/api/highlight", { syntax_theme: "other" }, ctx("codediff"));
  assert.deepEqual(highlight.json.before, [3]);
  assert.deepEqual(highlight.json.after, [4]);
  assert.equal(highlight.json.syntax.theme, "other");

  for (const path of ["/api/settings", "/api/cancel"]) {
    assert.deepEqual(S.routeApi(path, {}, ctx("diff")), { status: 200, json: {} });
  }
  for (const path of ["/api/quit", "/api/edit", "/api/ls", "/api/review", "/api/review/open"]) {
    const answer = S.routeApi(path, {}, ctx("diff"));
    assert.equal(answer.status, 400, path);
    assert.ok(answer.json.error, path);
  }
  assert.equal(S.routeApi("/api/nothing", {}, ctx("diff")).status, 404);
}

// ----- the strip's text -----------------------------------------------------------------------

{
  assert.equal(S.plural(1, "span"), "1 span");
  assert.equal(S.plural(0, "line"), "0 lines");
  assert.equal(S.formatCounts({ updates: 1, moves: 2, insertions: 0, deletions: 1 }), "1 update, 2 moves, 1 deletion");
  assert.equal(S.formatCounts({ updates: 0, moves: 0, insertions: 0, deletions: 0 }), "nothing");
  assert.equal(S.statsLine(cases[0]), "Unix diff marks 4 lines · CodeDiff marks 1 update");
  assert.equal(
    S.statsLine(cases[1]),
    "Unix diff marks 1 line · CodeDiff marks 1 move, 2 insertions (Whitespace changes only)",
  );
  assert.equal(
    S.firstPaintedRow({
      before: { ranges: [{ op: "identical", source: [0, 0, 9, 0] }, { op: "delete", source: [7, 0, 8, 0] }] },
      after: { ranges: [{ op: "insert", source: [3, 2, 3, 9] }] },
    }),
    3,
  );
  assert.equal(S.firstPaintedRow({ before: { ranges: [] }, after: { ranges: [{ op: "unset", source: [0, 0, 1, 0] }] } }), Infinity);
  assert.equal(S.GROUPS.length, 2);
  assert.deepEqual(S.VIEWS, ["diff", "codediff"]);
}

console.log("showcase.test.js: all assertions passed");
