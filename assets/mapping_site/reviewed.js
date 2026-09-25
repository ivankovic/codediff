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

// Vanilla-JS "I have reviewed this fixture" marks for the human_mapping static site. No
// framework, no build step - copied verbatim into every generated site by
// generate_mapping_site.rs, same convention as index.js and viewer.js. Unlike those two, this
// runs on *both* kinds of page: the index (where it fills a "Reviewed" column and a progress
// line) and every fixture page (where it owns the "Mark as reviewed" button). It is a third
// script rather than a copy in each because the storage format is the one thing the two pages
// must agree on exactly.
//
// The marks live in this browser's localStorage, nowhere else: the site is static and has no
// server to send them to. So they are per browser and per device, and nothing here syncs them.
//
// A mark records the *revision* it was made against - the fingerprint generate_mapping_site.rs
// bakes into each page from the mapping and the two source files - not just the fixture's name.
// The site is regenerated from src/test/data/diffs/ on every push, and a fixture that was remapped
// or repainted since it was reviewed has not really been reviewed: its mark shows as "stale", and
// the random-unreviewed picker treats it as unreviewed.
(function () {
  "use strict";

  // ─── Pure helpers ───────────────────────────────────────────────────────────────────────────
  //
  // Everything in this block takes what it needs as arguments instead of reading the document or
  // localStorage, so reviewed.test.js can drive it under plain Node.

  // Prefixed like the viewer's own keys: localStorage is shared by everything on this origin
  // (all of ivankovic.github.io), not just this site.
  const STORAGE_KEY = "codediff-mapping-reviewed";

  // The stored shape is one object of fixture name -> revision string. Anything else (an older
  // format, a hand-edited value, garbage) reads as "no marks" rather than throwing on every page.
  function parseMarks(json) {
    if (!json) return {};
    let parsed;
    try {
      parsed = JSON.parse(json);
    } catch (e) {
      return {};
    }
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) return {};
    const marks = {};
    for (const name of Object.keys(parsed)) {
      if (typeof parsed[name] === "string") marks[name] = parsed[name];
    }
    return marks;
  }

  function serializeMarks(marks) {
    return JSON.stringify(marks);
  }

  // "reviewed" when the mark was made against this very revision, "stale" when it was made against
  // an earlier one, "unreviewed" when there is no mark at all.
  function markState(marks, name, revision) {
    if (!Object.prototype.hasOwnProperty.call(marks, name)) return "unreviewed";
    return marks[name] === revision ? "reviewed" : "stale";
  }

  // Both return a new object: callers store what they get back, so the input is never mutated
  // behind a caller that is still comparing against it.
  function withMark(marks, name, revision) {
    const next = Object.assign({}, marks);
    next[name] = revision;
    return next;
  }

  function withoutMark(marks, name) {
    const next = Object.assign({}, marks);
    delete next[name];
    return next;
  }

  // Sort key for the index's "Reviewed" column: higher is more done, so ascending puts the work
  // that is left first.
  function sortValue(state) {
    return state === "reviewed" ? 2 : state === "stale" ? 1 : 0;
  }

  // One of `fixtures` (each `{ name, revision }`) that is not reviewed at this revision, chosen by
  // `random` (a function returning a number in [0, 1), `Math.random` in the browser - injected so
  // the test can pin the choice). `exclude` is the fixture the reader is already looking at, so
  // "another random one" never answers with the same page. Null when everything is reviewed.
  function pickUnreviewed(fixtures, marks, random, exclude) {
    const candidates = fixtures.filter(
      (fixture) =>
        fixture.name !== exclude &&
        markState(marks, fixture.name, fixture.revision) !== "reviewed"
    );
    if (candidates.length === 0) return null;
    const index = Math.min(candidates.length - 1, Math.floor(random() * candidates.length));
    return candidates[index];
  }

  function summarize(fixtures, marks) {
    const counts = { reviewed: 0, stale: 0, unreviewed: 0, total: fixtures.length };
    for (const fixture of fixtures) {
      counts[markState(marks, fixture.name, fixture.revision)]++;
    }
    return counts;
  }

  // Exposed for reviewed.test.js (plain Node, no DOM) - a no-op in the browser, where `module` is
  // undefined and this branch never runs.
  if (typeof module !== "undefined") {
    module.exports = {
      STORAGE_KEY,
      markState,
      parseMarks,
      pickUnreviewed,
      serializeMarks,
      sortValue,
      summarize,
      withMark,
      withoutMark,
    };
  }

  // Everything below this line drives the real page and needs a DOM - never runs under Node.
  if (typeof document === "undefined") return;

  // ─── Storage ────────────────────────────────────────────────────────────────────────────────
  //
  // Best-effort, like the viewer's remembered view: a page opened from a file:// URL, or with
  // storage disabled, still works - the marks just do not survive the page.

  function loadMarks() {
    try {
      return parseMarks(window.localStorage.getItem(STORAGE_KEY));
    } catch (e) {
      return {};
    }
  }

  function storeMarks(marks) {
    try {
      window.localStorage.setItem(STORAGE_KEY, serializeMarks(marks));
    } catch (e) {
      /* ignore */
    }
  }

  // ─── The fixture list ───────────────────────────────────────────────────────────────────────
  //
  // The index page has it as its table rows. A fixture page has no table, so it loads
  // assets/fixtures.js, which the generator writes as one array on `window`; it is the same
  // names and revisions the index bakes into its rows.

  const table = document.getElementById("fixture-table");
  const tableRows = table ? Array.from(table.querySelectorAll("tbody tr")) : [];

  function fixtureList() {
    if (tableRows.length > 0) {
      return tableRows.map((row) => ({
        name: row.dataset.name || "",
        revision: row.dataset.revision || "",
      }));
    }
    return Array.isArray(window.CODEDIFF_FIXTURES) ? window.CODEDIFF_FIXTURES : [];
  }

  // ─── Random unreviewed fixture ──────────────────────────────────────────────────────────────
  //
  // Present on both kinds of page; the button says where the fixture pages are relative to it.

  const randomButton = document.getElementById("random-unreviewed");
  const currentFixture = document.body.dataset.fixture || "";

  function openRandomUnreviewed() {
    const picked = pickUnreviewed(fixtureList(), loadMarks(), Math.random, currentFixture);
    if (!picked) {
      setStatus("Every fixture is marked as reviewed at its current revision");
      return;
    }
    const dir = randomButton ? randomButton.dataset.fixturesDir || "" : "";
    window.location.href = dir + picked.name + ".html";
  }

  if (randomButton) randomButton.addEventListener("click", openRandomUnreviewed);

  // The fixture page's footer status line, if this is a fixture page; the index has none, and a
  // message with nowhere to go is dropped.
  const statusLine = document.getElementById("status-line");
  function setStatus(message) {
    if (statusLine) statusLine.textContent = message;
  }

  // ─── The fixture page: one toggle button ────────────────────────────────────────────────────

  const toggleButton = document.getElementById("toggle-reviewed");
  const currentRevision = document.body.dataset.revision || "";

  function renderToggle() {
    if (!toggleButton) return;
    const state = markState(loadMarks(), currentFixture, currentRevision);
    toggleButton.dataset.state = state;
    toggleButton.setAttribute("aria-pressed", state === "reviewed" ? "true" : "false");
    toggleButton.textContent =
      state === "reviewed"
        ? "Reviewed ✓"
        : state === "stale"
          ? "Reviewed before this mapping changed – mark again"
          : "Mark as reviewed";
  }

  function toggleReviewed() {
    if (!currentFixture) return;
    const marks = loadMarks();
    const state = markState(marks, currentFixture, currentRevision);
    // A stale mark is re-made, not removed: the reader has just looked at the new revision.
    if (state === "reviewed") {
      storeMarks(withoutMark(marks, currentFixture));
      setStatus("No longer marked as reviewed");
    } else {
      storeMarks(withMark(marks, currentFixture, currentRevision));
      setStatus("Marked as reviewed (in this browser only)");
    }
    renderToggle();
  }

  if (toggleButton) {
    toggleButton.addEventListener("click", toggleReviewed);
    renderToggle();

    // viewer.js owns the navigation keys and lets any it does not know fall through, so these two
    // can live here. Its search prompt is a focused <input>, which is why typing there must not
    // toggle anything; and a modifier means a browser shortcut (Ctrl+R is reload), not ours.
    document.addEventListener("keydown", (event) => {
      if (event.ctrlKey || event.metaKey || event.altKey) return;
      const target = event.target;
      if (target && (target.tagName === "INPUT" || target.tagName === "TEXTAREA")) return;
      if (event.key === "r") {
        toggleReviewed();
      } else if (event.key === "n") {
        openRandomUnreviewed();
      }
    });
  }

  // ─── The index page: a column and a progress line ──────────────────────────────────────────

  const progressLine = document.getElementById("review-progress");

  function renderIndex() {
    if (tableRows.length === 0) return;
    const marks = loadMarks();
    for (const row of tableRows) {
      const state = markState(marks, row.dataset.name || "", row.dataset.revision || "");
      // What index.js sorts the column by; baked as 0 so the column sorts even without storage.
      row.dataset.reviewed = String(sortValue(state));
      const box = row.querySelector("input.reviewed-mark");
      if (!box) continue;
      box.checked = state === "reviewed";
      box.indeterminate = state === "stale";
      box.title =
        state === "reviewed"
          ? "Reviewed at its current revision; untick to forget"
          : state === "stale"
            ? "Reviewed, but the mapping has changed since; tick to mark the current revision"
            : "Not reviewed";
    }
    if (progressLine) {
      const counts = summarize(fixtureList(), marks);
      let text = `${counts.reviewed} of ${counts.total} marked as reviewed in this browser`;
      if (counts.stale > 0) {
        text += `, ${counts.stale} reviewed before the mapping changed`;
      }
      progressLine.textContent = text + ".";
    }
  }

  for (const row of tableRows) {
    const box = row.querySelector("input.reviewed-mark");
    if (!box) continue;
    box.addEventListener("change", () => {
      const marks = loadMarks();
      const name = row.dataset.name || "";
      const revision = row.dataset.revision || "";
      // A stale box reads as unchecked to the browser, so ticking it marks the current revision -
      // the same "re-make, don't remove" rule as the fixture page's button.
      storeMarks(
        box.checked ? withMark(marks, name, revision) : withoutMark(marks, name)
      );
      renderIndex();
    });
  }

  renderIndex();
})();
