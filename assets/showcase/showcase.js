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

// The showcase's half of the page: a `fetch` shim that answers the viewer's `/api/*` calls from
// the JSON `generate_showcase` baked, and the strip of chrome above the viewer that picks a case
// and flips between `diff`'s view and codediff's. app.js and model.js are the viewer under
// assets/viewer/, loaded unchanged; nothing here reaches into them. Switching a case or a view
// rewrites what the shim will answer and then presses the viewer's own `r` (reload), which makes
// it ask again.
//
// Loaded before app.js, whose `init()` runs at load and asks `/api/state` first - so the shim
// must be installed synchronously here, and it is; the data it answers with is fetched lazily
// behind a promise the first `/api/` call awaits.
//
// The pure parts (URL <-> selection, the API routing table) are exported for
// showcase.test.js, run by `make test-showcase-js`; the DOM wiring is not.
"use strict";

const CodeDiffShowcase = (() => {
  const GROUPS = [
    {
      id: "diff_wrong",
      label: "Where diff gets it wrong and CodeDiff is exact",
      note:
        "On each of these, Unix diff marks whole lines a reader then has to diff again by eye, " +
        "and CodeDiff's mapping agrees with the human one line for line.",
    },
    {
      id: "both_right",
      label: "Where both are right",
      note:
        "On each of these a plain line diff is already the right answer, and CodeDiff says the " +
        "same thing. A syntax-aware tool must not invent structure where there is none.",
    },
  ];
  const VIEWS = ["diff", "codediff"];
  const VIEW_LABEL = { diff: "Unix diff", codediff: "CodeDiff" };

  // ----- URL <-> selection ------------------------------------------------------------------

  // `?case=<name>&view=diff|codediff`. Anything missing or unknown falls back to the first case
  // and to `diff` - the view a newcomer already knows, so the flip to CodeDiff is the reveal.
  function parseQuery(search, cases) {
    const params = new URLSearchParams(search || "");
    const wanted = params.get("case");
    const known = cases.find((c) => c.name === wanted);
    const view = params.get("view");
    return {
      name: known ? known.name : cases.length ? cases[0].name : null,
      view: VIEWS.includes(view) ? view : "diff",
    };
  }

  function buildQuery(name, view) {
    const params = new URLSearchParams();
    params.set("case", name);
    params.set("view", view);
    return `?${params.toString()}`;
  }

  // ----- the API the page thinks it is talking to ------------------------------------------

  // Which baked payload answers for a (view, render options) pair. `diff`'s view has one
  // rendering; codediff's has the default and the two presets the `M` panel can pick, and any
  // other combination of options gets the default rather than a recomputation nobody can run.
  function payloadFor(view, payloads, presets, options) {
    if (view === "diff") return payloads.diff;
    if (options && presets) {
      if (sameOptions(options, presets.minimal)) return payloads.minimal;
      if (sameOptions(options, presets.full)) return payloads.full;
    }
    return payloads.codediff;
  }

  function sameOptions(a, b) {
    const keys = new Set([...Object.keys(a), ...Object.keys(b)]);
    for (const key of keys) if (a[key] !== b[key]) return false;
    return true;
  }

  // One request in, one `{status, json}` out. `ctx` is what the page has selected and loaded:
  // `state` (state.json), `view`, `payloads` (the four JSONs of the selected case), `presets`
  // (from state.json) and `options` (the last render options the page sent, if any).
  function routeApi(path, body, ctx) {
    const ok = (json) => ({ status: 200, json });
    const refuse = (message) => ({ status: 400, json: { error: message } });
    switch (path) {
      case "/api/state":
        return ok({ ...ctx.state, before: "before", after: "after" });
      case "/api/diff":
        return ok(payloadFor(ctx.view, ctx.payloads, ctx.presets, ctx.options));
      case "/api/render_options": {
        ctx.options = body;
        const payload = payloadFor(ctx.view, ctx.payloads, ctx.presets, body);
        // The page shows what it asked for in its footer badge; on `diff`'s view every option
        // paints the same whole lines, so echoing the request back is honest there too.
        return ok({ ...payload, render_options: body });
      }
      case "/api/highlight": {
        const payload = payloadFor(ctx.view, ctx.payloads, ctx.presets, ctx.options);
        // Only the theme the site was baked with exists; a different name gets the same spans
        // under its own name, which keeps the page consistent rather than half-recoloured.
        return ok({
          before: payload.before.spans,
          after: payload.after.spans,
          syntax: { ...payload.syntax, theme: (body && body.syntax_theme) || payload.syntax.theme },
        });
      }
      case "/api/settings":
      case "/api/cancel":
        return ok({});
      case "/api/quit":
        return refuse("This page never quits. Pick another change above.");
      case "/api/edit":
        return refuse("Editing is not available in the showcase.");
      case "/api/ls":
      case "/api/review":
      case "/api/review/open":
        return refuse("Only the changes listed above can be opened here. Install codediff to diff your own.");
      default:
        return { status: 404, json: { error: `no such endpoint: ${path}` } };
    }
  }

  // The first row either side paints, or Infinity when nothing is painted - what decides whether
  // a freshly loaded diff opens on line 1 or jumps to its first change.
  function firstPaintedRow(payload) {
    let first = Infinity;
    for (const side of [payload.before, payload.after]) {
      for (const range of (side && side.ranges) || []) {
        if (range.op !== "identical" && range.op !== "unset") first = Math.min(first, range.source[0]);
      }
    }
    return first;
  }

  function plural(n, word) {
    return `${n} ${word}${n === 1 ? "" : "s"}`;
  }

  function formatCounts(counts) {
    const parts = [];
    if (counts.updates) parts.push(plural(counts.updates, "update"));
    if (counts.moves) parts.push(plural(counts.moves, "move"));
    if (counts.insertions) parts.push(plural(counts.insertions, "insertion"));
    if (counts.deletions) parts.push(plural(counts.deletions, "deletion"));
    return parts.length ? parts.join(", ") : "nothing";
  }

  function statsLine(entry) {
    const summary = entry.summary ? ` (${entry.summary})` : "";
    return (
      `Unix diff marks ${plural(entry.diff_marked, "line")} · ` +
      `CodeDiff marks ${formatCounts(entry.codediff)}${summary}`
    );
  }

  return {
    GROUPS,
    VIEWS,
    VIEW_LABEL,
    parseQuery,
    buildQuery,
    payloadFor,
    routeApi,
    firstPaintedRow,
    plural,
    formatCounts,
    statsLine,
  };
})();

if (typeof module !== "undefined") {
  module.exports = CodeDiffShowcase;
} else {
  (() => {
    const S = CodeDiffShowcase;
    const $ = (selector) => document.querySelector(selector);
    // Rows visible without scrolling on a typical window, roughly; a first change past this many
    // rows is one the reader would otherwise not see on load.
    const FOLD_ROWS = 14;
    const realFetch = window.fetch.bind(window);

    const ctx = { state: null, view: "diff", payloads: null, presets: null, options: null };
    let cases = [];
    let selected = null;
    const cache = new Map();
    // Whether the next diff the page loads should land on its first change rather than on line
    // 1: yes on the first load and on a change of case, no on a flip between views, where the
    // viewer restores the reader's own cursor and that is the point of flipping.
    let jumpToFirstChange = true;

    const ready = Promise.all([
      realFetch("cases.json").then((r) => r.json()),
      realFetch("state.json").then((r) => r.json()),
    ]).then(([list, state]) => {
      cases = list;
      ctx.state = state;
      ctx.presets = state.presets;
      const initial = S.parseQuery(window.location.search, cases);
      selected = initial.name;
      ctx.view = initial.view;
      buildChrome();
      showSelection();
    });

    async function loadCase(name) {
      if (!cache.has(name)) {
        const load = (suffix) => realFetch(`cases/${name}.${suffix}.json`).then((r) => r.json());
        cache.set(
          name,
          Promise.all([load("diff"), load("codediff"), load("minimal"), load("full")]).then(
            ([diff, codediff, minimal, full]) => ({ diff, codediff, minimal, full }),
          ),
        );
      }
      return cache.get(name);
    }

    // The shim. Anything outside /api/ (the JSON files themselves) goes to the network as usual.
    window.fetch = async (path, init) => {
      if (typeof path !== "string" || !path.startsWith("/api/")) return realFetch(path, init);
      await ready;
      ctx.payloads = await loadCase(selected);
      let body = {};
      try {
        body = init && init.body ? JSON.parse(init.body) : {};
      } catch (_) {
        body = {};
      }
      const { status, json } = S.routeApi(path, body, ctx);
      if (path === "/api/diff") {
        const jump = jumpToFirstChange && S.firstPaintedRow(json) >= FOLD_ROWS;
        jumpToFirstChange = false;
        setTimeout(() => {
          updateTitle();
          // The viewer's own "next change" from line 1, once the payload has been painted:
          // a change forty lines down would otherwise open on an unchanged screen.
          if (jump) document.dispatchEvent(new KeyboardEvent("keydown", { key: "n", bubbles: true }));
        }, 150);
      }
      return new Response(JSON.stringify(json), {
        status,
        headers: { "Content-Type": "application/json" },
      });
    };

    // ----- chrome -----------------------------------------------------------------------------

    function buildChrome() {
      const select = $("#case-select");
      select.innerHTML = "";
      for (const group of S.GROUPS) {
        const optgroup = document.createElement("optgroup");
        optgroup.label = group.label;
        for (const entry of cases.filter((c) => c.group === group.id)) {
          const option = document.createElement("option");
          option.value = entry.name;
          option.textContent = `${entry.title} (${entry.language})`;
          optgroup.append(option);
        }
        select.append(optgroup);
      }
      select.addEventListener("change", () => select_(select.value, ctx.view));
      $("#view-diff").addEventListener("click", () => select_(selected, "diff"));
      $("#view-codediff").addEventListener("click", () => select_(selected, "codediff"));
      // `q` and Escape quit the real viewer. Here they would leave the reader on a dead screen,
      // so at the top level (no dialog open) they are swallowed; inside a dialog they still close
      // it, which is app.js's own handling.
      document.addEventListener(
        "keydown",
        (event) => {
          const dialogOpen = !$("#overlay").hidden;
          if ((event.key === "q" || event.key === "Escape") && !dialogOpen && !isTyping(event)) {
            event.stopImmediatePropagation();
            event.preventDefault();
          }
        },
        true,
      );
    }

    function isTyping(event) {
      const tag = event.target && event.target.tagName;
      return tag === "INPUT" || tag === "SELECT" || tag === "TEXTAREA";
    }

    function select_(name, view) {
      const changed = name !== selected || view !== ctx.view;
      if (name !== selected) jumpToFirstChange = true;
      selected = name;
      ctx.view = view;
      ctx.options = null;
      showSelection();
      if (!changed) return;
      // The viewer's own reload: it asks `/api/diff` again, and the shim now answers differently.
      // Focus leaves the select first so the keystroke reaches the document, not the control.
      $("#case-select").blur();
      document.dispatchEvent(new KeyboardEvent("keydown", { key: "r", bubbles: true }));
    }

    function showSelection() {
      const entry = cases.find((c) => c.name === selected);
      if (!entry) return;
      window.history.replaceState(null, "", S.buildQuery(selected, ctx.view));
      $("#case-select").value = selected;
      for (const view of S.VIEWS) {
        const button = $(`#view-${view}`);
        const active = view === ctx.view;
        button.classList.toggle("active", active);
        button.setAttribute("aria-pressed", String(active));
      }
      $("#case-stats").textContent = S.statsLine(entry);
      const group = S.GROUPS.find((g) => g.id === entry.group);
      $("#case-blurb").innerHTML = "";
      const strong = document.createElement("strong");
      strong.textContent = entry.title + ". ";
      $("#case-blurb").append(strong, entry.blurb, " ");
      const note = document.createElement("span");
      note.className = "group-note";
      note.textContent = group ? group.note : "";
      $("#case-blurb").append(note);
      const links = $("#case-links");
      links.innerHTML = "";
      const mapping = document.createElement("a");
      mapping.href = entry.mapping;
      mapping.textContent = "Human mapping for this change";
      links.append(mapping);
      if (entry.upstream) {
        links.append(" · ");
        const commit = document.createElement("a");
        commit.href = entry.upstream;
        commit.textContent = "Upstream commit";
        links.append(commit);
      } else {
        links.append(" · hand-written example");
      }
      links.append(" · ");
      updateTitle();
    }

    function updateTitle() {
      const entry = cases.find((c) => c.name === selected);
      if (entry) document.title = `${entry.title} · ${S.VIEW_LABEL[ctx.view]} · CodeDiff live examples`;
    }
  })();
}
