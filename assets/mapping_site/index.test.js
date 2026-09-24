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

// Plain-Node regression test for index.js's sorting logic - no framework, no npm dependency,
// consistent with this directory's own "no framework, no build step" convention (see index.js's
// header comment). Run directly: `node assets/mapping_site/index.test.js` (also wired into
// `make test` and CI - see Makefile/`.github/workflows/ci.yml`).
"use strict";

const assert = require("assert");
const { cellValue, compareRows } = require("./index.js");

// The actual bug this file exists to catch: `dataset` only camelCases hyphens, not underscores, so
// a real `data-total_lines="123"` attribute reads back as `dataset.total_lines`, not
// `dataset.totalLines`. An earlier version of `cellValue` converted "total_lines" to "totalLines"
// before the lookup, silently reading `undefined` (-> `NaN`) for every row on that column (and
// `unix_diff`, the other underscored one) - see index.js's own comment on `cellValue`.
{
  const row = {
    dataset: { name: "foo", codediff: "3", unix_diff: "7", total_lines: "123" },
  };
  assert.strictEqual(cellValue(row, "name", "string"), "foo");
  assert.strictEqual(cellValue(row, "codediff", "number"), 3);
  assert.strictEqual(cellValue(row, "unix_diff", "number"), 7);
  assert.strictEqual(cellValue(row, "total_lines", "number"), 123);
}

// `compareRows` must actually produce a usable ascending/descending ordering for every column,
// including the two that were broken (`unix_diff`, `total_lines`) - `Number(undefined)` is `NaN`,
// and `NaN - NaN` is also `NaN`, which `Array.prototype.sort` treats as "leave these in place"
// (never reorders), so a regression here would silently report "sorted" without throwing, exactly
// like the reported bug. Asserting the actual output order (not just that `cellValue` returns the
// right numbers, the earlier check above) is what catches that class of failure.
{
  const rows = [
    { dataset: { name: "c", codediff: "1", unix_diff: "9", total_lines: "300" } },
    { dataset: { name: "a", codediff: "3", unix_diff: "1", total_lines: "100" } },
    { dataset: { name: "b", codediff: "2", unix_diff: "5", total_lines: "200" } },
  ];

  const columns = [
    ["name", "string"],
    ["codediff", "number"],
    ["unix_diff", "number"],
    ["total_lines", "number"],
  ];

  for (const [key, type] of columns) {
    const ascending = [...rows].sort((a, b) => compareRows(a, b, key, type, true));
    const ascendingValues = ascending.map((r) => cellValue(r, key, type));
    const expectedAscending = [...ascendingValues].sort((a, b) =>
      type === "number" ? a - b : String(a).localeCompare(String(b)),
    );
    assert.deepStrictEqual(
      ascendingValues,
      expectedAscending,
      `${key} (ascending) did not sort: ${JSON.stringify(ascendingValues)}`,
    );

    const descending = [...rows].sort((a, b) => compareRows(a, b, key, type, false));
    const descendingValues = descending.map((r) => cellValue(r, key, type));
    assert.deepStrictEqual(
      descendingValues,
      [...expectedAscending].reverse(),
      `${key} (descending) did not sort: ${JSON.stringify(descendingValues)}`,
    );
  }
}

console.log("index.test.js: all assertions passed");
