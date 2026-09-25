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

// Plain-Node regression test for reviewed.js's logic - no framework, no npm dependency, no DOM,
// consistent with this directory's own "no framework, no build step" convention (see index.js's
// header comment). Run directly: `node assets/mapping_site/reviewed.test.js` (also wired into
// `make test-mapping-site-js` and CI - see Makefile/`.github/workflows/ci.yml`).
//
// What it pins is the part where a bug is silent: a mark that survives a remapping, a stored blob
// that stops parsing and takes every mark with it, or a random pick that can land on a fixture
// already reviewed (or the one the reader is on).
"use strict";

const assert = require("assert");
const {
  markState,
  parseMarks,
  pickUnreviewed,
  serializeMarks,
  sortValue,
  summarize,
  withMark,
  withoutMark,
} = require("./reviewed.js");

// ── A mark is only "reviewed" at the revision it was made against ─────────────────────────────
{
  const marks = withMark({}, "rust-add-if", "rev1");
  assert.strictEqual(markState(marks, "rust-add-if", "rev1"), "reviewed");
  assert.strictEqual(markState(marks, "rust-add-if", "rev2"), "stale");
  assert.strictEqual(markState(marks, "python-other", "rev1"), "unreviewed");
  assert.strictEqual(markState(withoutMark(marks, "rust-add-if"), "rust-add-if", "rev1"), "unreviewed");
}

// ── withMark/withoutMark return new objects; the input is untouched ───────────────────────────
{
  const original = { a: "r1" };
  const added = withMark(original, "b", "r2");
  const removed = withoutMark(added, "a");
  assert.deepStrictEqual(original, { a: "r1" });
  assert.deepStrictEqual(added, { a: "r1", b: "r2" });
  assert.deepStrictEqual(removed, { b: "r2" });
}

// ── A fixture named like an Object.prototype property is still just a name ────────────────────
{
  assert.strictEqual(markState({}, "constructor", "r1"), "unreviewed");
  assert.strictEqual(markState({}, "hasOwnProperty", "r1"), "unreviewed");
}

// ── Round trip through the stored string, and tolerance of anything that isn't one ────────────
{
  const marks = { "rust-add-if": "rev1", "python-other": "rev9" };
  assert.deepStrictEqual(parseMarks(serializeMarks(marks)), marks);

  assert.deepStrictEqual(parseMarks(null), {});
  assert.deepStrictEqual(parseMarks(""), {});
  assert.deepStrictEqual(parseMarks("not json"), {});
  assert.deepStrictEqual(parseMarks("[1,2]"), {});
  assert.deepStrictEqual(parseMarks("42"), {});
  // A value that is not a revision string is dropped, the rest kept.
  assert.deepStrictEqual(parseMarks('{"a":"r1","b":true,"c":{"x":1}}'), { a: "r1" });
}

// ── Sort key: ascending puts the remaining work first ─────────────────────────────────────────
{
  assert.ok(sortValue("unreviewed") < sortValue("stale"));
  assert.ok(sortValue("stale") < sortValue("reviewed"));
}

// ── The random pick never lands on a reviewed fixture, nor on the current page ────────────────
{
  const fixtures = [
    { name: "a", revision: "r1" },
    { name: "b", revision: "r1" },
    { name: "c", revision: "r1" },
    { name: "d", revision: "r1" },
  ];
  const marks = { a: "r1", c: "old" };

  // Every point of the random range is checked: only b (unreviewed) and c (stale) may come up.
  for (const r of [0, 0.25, 0.5, 0.75, 0.999]) {
    const picked = pickUnreviewed(fixtures, marks, () => r, "d");
    assert.ok(picked && (picked.name === "b" || picked.name === "c"), `picked ${JSON.stringify(picked)} at ${r}`);
  }
  // Both candidates are reachable.
  assert.strictEqual(pickUnreviewed(fixtures, marks, () => 0, "d").name, "b");
  assert.strictEqual(pickUnreviewed(fixtures, marks, () => 0.99, "d").name, "c");
  // Without an exclusion, d is a candidate too.
  assert.strictEqual(pickUnreviewed(fixtures, marks, () => 0.99, "").name, "d");
  // A `random` that returns exactly 1 (it must not, but a stand-in might) still picks something.
  assert.ok(pickUnreviewed(fixtures, marks, () => 1, "d"));

  // Everything reviewed: nothing to pick.
  assert.strictEqual(pickUnreviewed(fixtures, { a: "r1", b: "r1", c: "r1", d: "r1" }, () => 0, ""), null);
  // The only unreviewed fixture is the one being excluded: still nothing.
  assert.strictEqual(pickUnreviewed(fixtures, { a: "r1", b: "r1", c: "r1" }, () => 0, "d"), null);
  assert.strictEqual(pickUnreviewed([], {}, () => 0, ""), null);
}

// ── The progress line's counts ────────────────────────────────────────────────────────────────
{
  const fixtures = [
    { name: "a", revision: "r1" },
    { name: "b", revision: "r1" },
    { name: "c", revision: "r1" },
  ];
  assert.deepStrictEqual(summarize(fixtures, { a: "r1", c: "old" }), {
    reviewed: 1,
    stale: 1,
    unreviewed: 1,
    total: 3,
  });
  assert.deepStrictEqual(summarize([], {}), { reviewed: 0, stale: 0, unreviewed: 0, total: 0 });
}

console.log("reviewed.test.js: all assertions passed");
