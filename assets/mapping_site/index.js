// Vanilla-JS click-to-sort for the human_mapping index page's fixture table. No framework, no
// build step - copied verbatim into every generated site by generate_mapping_site.rs, same
// convention as viewer.js. Deliberately separate from viewer.js rather than folded into it: this
// only ever runs on index.html (which never loads viewer.js - there's no before/after tree, no
// mapping to navigate), and viewer.js only ever runs on a fixture page (which has no table to
// sort), so the two scripts' code never needs to coexist in the same page.
(function () {
  "use strict";

  const table = document.getElementById("fixture-table");
  if (!table) return;
  const tbody = table.querySelector("tbody");
  const headers = Array.from(table.querySelectorAll("th[data-sort]"));

  // The table already renders sorted by fixture name (generate_mapping_site.rs sorts `names`
  // before building rows) - starting state matches the "name" header's own `aria-sort="ascending"`
  // baked into the HTML, so the two don't have to be kept in sync by hand beyond this one line.
  let currentKey = "name";
  let ascending = true;

  function cellValue(row, key, type) {
    const raw = row.dataset[toCamelCase(key)];
    return type === "number" ? Number(raw) : raw;
  }

  // `data-total_lines` -> `dataset.totalLines`: the DOM's own snake_case-to-camelCase dataset
  // mapping, replicated here since `key` comes from a `data-sort` attribute value (plain
  // "total_lines"), not from reading the dataset back.
  function toCamelCase(key) {
    return key.replace(/_([a-z])/g, (_, c) => c.toUpperCase());
  }

  function sortBy(key, type) {
    if (key === currentKey) {
      ascending = !ascending;
    } else {
      currentKey = key;
      ascending = true;
    }

    const rows = Array.from(tbody.querySelectorAll("tr"));
    rows.sort((a, b) => {
      const av = cellValue(a, key, type);
      const bv = cellValue(b, key, type);
      const cmp = type === "number" ? av - bv : String(av).localeCompare(String(bv));
      return ascending ? cmp : -cmp;
    });
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
