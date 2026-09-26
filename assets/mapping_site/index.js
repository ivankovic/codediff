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

// Vanilla-JS click-to-sort for the human_mapping index page's fixture table. No framework, no
// build step - copied verbatim into every generated site by generate_mapping_site.rs, same
// convention as viewer.js. Deliberately separate from viewer.js rather than folded into it: this
// only ever runs on index.html (which never loads viewer.js - there's no before/after tree, no
// mapping to navigate), and viewer.js only ever runs on a fixture page (which has no table to
// sort), so the two scripts' code never needs to coexist in the same page. What both pages do
// share - the reader's "reviewed" marks - lives in reviewed.js, which both load.
(function () {
  "use strict";

  // `dataset` camelCases only hyphens, so `data-total_lines` reads back as `dataset.total_lines` -
  // the form a `data-sort` key already has. Converting its case would read `undefined` for every
  // underscored key and silently stop those columns sorting. Pinned by index.test.js.
  function cellValue(row, key, type) {
    const raw = row.dataset[key];
    return type === "number" ? Number(raw) : raw;
  }

  // The actual row-vs-row ordering `sortBy` sorts by - factored out (rather than left as an inline
  // arrow in `Array.prototype.sort`) so index.test.js can drive it directly against plain
  // `{ dataset: {...} }` objects, without needing a real DOM (or a fake one) to prove rows actually
  // reorder, not just that `cellValue` extracts the right value.
  function compareRows(a, b, key, type, ascending) {
    const av = cellValue(a, key, type);
    const bv = cellValue(b, key, type);
    const cmp = type === "number" ? av - bv : String(av).localeCompare(String(bv));
    return ascending ? cmp : -cmp;
  }

  // Exposed for index.test.js (plain Node, no DOM, no npm dependency) - a no-op in the browser,
  // where `module` is undefined and this branch never runs.
  if (typeof module !== "undefined") {
    module.exports = { cellValue, compareRows };
  }

  // Everything below this line drives the real page and needs a DOM - never runs under Node.
  if (typeof document === "undefined") return;

  const table = document.getElementById("fixture-table");
  if (!table) return;
  const tbody = table.querySelector("tbody");
  const headers = Array.from(table.querySelectorAll("th[data-sort]"));

  // The table already renders sorted by fixture name (generate_mapping_site.rs sorts `names`
  // before building rows) - starting state matches the "name" header's own `aria-sort="ascending"`
  // baked into the HTML, so the two don't have to be kept in sync by hand beyond this one line.
  let currentKey = "name";
  let ascending = true;

  function sortBy(key, type) {
    if (key === currentKey) {
      ascending = !ascending;
    } else {
      currentKey = key;
      ascending = true;
    }

    const rows = Array.from(tbody.querySelectorAll("tr"));
    rows.sort((a, b) => compareRows(a, b, key, type, ascending));
    for (const row of rows) tbody.appendChild(row);

    for (const th of headers) {
      th.setAttribute("aria-sort", th.dataset.sort === key ? (ascending ? "ascending" : "descending") : "none");
    }
  }

  for (const th of headers) {
    th.addEventListener("click", () => sortBy(th.dataset.sort, th.dataset.type));
    th.addEventListener("keydown", (event) => {
      if (event.key === "Enter" || event.key === " ") {
        event.preventDefault();
        sortBy(th.dataset.sort, th.dataset.type);
      }
    });
  }
})();
