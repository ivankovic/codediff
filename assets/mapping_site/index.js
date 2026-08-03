// Vanilla-JS click-to-sort for the human_mapping index page's fixture table. No framework, no
// build step - copied verbatim into every generated site by generate_mapping_site.rs, same
// convention as viewer.js. Deliberately separate from viewer.js rather than folded into it: this
// only ever runs on index.html (which never loads viewer.js - there's no before/after tree, no
// mapping to navigate), and viewer.js only ever runs on a fixture page (which has no table to
// sort), so the two scripts' code never needs to coexist in the same page.
(function () {
  "use strict";

  // `dataset` only camelCases *hyphens* (`data-foo-bar` -> `dataset.fooBar`) - an underscore in
  // the attribute name is left exactly as-is, so `data-total_lines` reads back as
  // `dataset.total_lines`, not `dataset.totalLines`. `key` (from a `data-sort` attribute value,
  // e.g. "total_lines") already matches that untouched form, so no case conversion is needed at
  // all - reaching for one here (an earlier version of this file did) reads back `undefined` for
  // every underscored key, breaking those columns' sort silently (`Number(undefined)` is `NaN`,
  // and every comparison against `NaN` is `false`, so the rows never actually reorder). Regression
  // test: index.test.js.
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
