// Vanilla-JS navigation for the human_mapping static viewer. No framework, no build step - copied
// verbatim into every generated site by generate_mapping_site.rs. Ported from a subset of
// human_solver's own keybindings (see that binary's top-of-file doc comment): only the read-only
// cursor-navigation ones (j/k/h/l/g/G/Tab/a/i//?) - nothing that mutates a mapping, and nothing
// that compares against codediff's own diff, since this viewer never runs codediff at all.
(function () {
  "use strict";

  const SIDES = ["before", "after"];
  const panels = {
    before: document.querySelector('.panel[data-side="before"]'),
    after: document.querySelector('.panel[data-side="after"]'),
  };
  const statusLine = document.getElementById("status-line");
  const issueLink = document.getElementById("file-issue");
  const toggleIdenticalButton = document.getElementById("toggle-identical");
  const helpOverlay = document.getElementById("help-overlay");
  const fixtureName = document.body.dataset.fixture || "";
  const repo = document.body.dataset.repo || "";

  let focusedSide = "before";
  const selected = { before: null, after: null };
  let counterpartEl = null;
  let searchQuery = { before: "", after: "" };

  function visibleNodes(side) {
    const panel = panels[side];
    if (!panel) return [];
    return Array.from(panel.querySelectorAll(".node")).filter(
      (el) => el.getClientRects().length > 0
    );
  }

  function setStatus(message) {
    statusLine.textContent = message || "";
  }

  function otherSide(side) {
    return side === "before" ? "after" : "before";
  }

  function clearCounterpartHighlight() {
    if (counterpartEl) {
      counterpartEl.classList.remove("counterpart");
      counterpartEl = null;
    }
  }

  function select(side, el, opts) {
    opts = opts || {};
    if (!el) return;

    const previous = selected[side];
    if (previous) previous.classList.remove("selected");
    el.classList.add("selected");
    selected[side] = el;

    if (opts.setFocus !== false) {
      setFocusedSide(side, { select: false });
    }

    if (opts.scroll !== false) {
      el.scrollIntoView({ block: "center" });
    }

    clearCounterpartHighlight();
    const matchId = el.dataset.match;
    if (matchId) {
      const match = document.getElementById(matchId);
      if (match) {
        match.classList.add("counterpart");
        counterpartEl = match;
        if (opts.scrollCounterpart) {
          match.scrollIntoView({ block: "center" });
        }
      }
    }

    updateIssueLink(side, el);
  }

  function setFocusedSide(side, opts) {
    opts = opts || {};
    focusedSide = side;
    for (const s of SIDES) {
      if (panels[s]) panels[s].dataset.focused = s === side ? "true" : "false";
    }
    if (opts.select !== false && !selected[side]) {
      const nodes = visibleNodes(side);
      if (nodes.length > 0) select(side, nodes[0], { setFocus: false, scroll: false });
    }
  }

  // The generator deliberately doesn't bake a `data-path` string into every node (it measurably
  // added to page size across ~thousands of nodes) - computed here instead, lazily, only for the
  // one node a click actually needs it for. Mirrors codediff's own `path_for_node` (Rust):
  // "kind:occurrence" per level, walking up to the tree root, where occurrence is the 1-indexed
  // count of same-kind `.node` elements at that level appearing at or before this one.
  function nodePath(el) {
    const segments = [];
    let current = el;
    while (true) {
      const parent = current.parentElement;
      if (!parent || !parent.classList.contains("node")) break;
      const siblings = Array.from(parent.children).filter((c) => c.classList.contains("node"));
      const kind = current.dataset.kind || "";
      let occurrence = 0;
      for (const sibling of siblings) {
        if (sibling === current) break;
        if (sibling.dataset.kind === kind) occurrence++;
      }
      segments.push(`${kind}:${occurrence + 1}`);
      current = parent;
    }
    segments.reverse();
    return segments.join("/");
  }

  function updateIssueLink(side, el) {
    const kind = el.dataset.kind || "";
    const path = nodePath(el);
    let status = "unmarked";
    for (const cls of el.classList) {
      if (cls.startsWith("status-")) status = cls.slice("status-".length);
    }

    const title = `human_mapping: ${fixtureName} - ${side} ${kind}`;
    const body = [
      `Fixture: ${fixtureName}`,
      `Side: ${side}`,
      `Node kind: ${kind}`,
      `Path: ${path || "(root)"}`,
      `Current status: ${status}`,
      "",
      "Describe why you disagree with this mapping:",
    ].join("\n");

    const url =
      `https://github.com/${repo}/issues/new` +
      `?title=${encodeURIComponent(title)}` +
      `&body=${encodeURIComponent(body)}` +
      `&labels=${encodeURIComponent("human-mapping")}`;

    issueLink.href = url;
    issueLink.removeAttribute("aria-disabled");
  }

  function moveCursor(side, delta) {
    const nodes = visibleNodes(side);
    if (nodes.length === 0) return;
    let idx = nodes.indexOf(selected[side]);
    if (idx === -1) idx = 0;
    idx = Math.max(0, Math.min(nodes.length - 1, idx + delta));
    select(side, nodes[idx]);
  }

  function jumpEdge(side, which) {
    const nodes = visibleNodes(side);
    if (nodes.length === 0) return;
    select(side, which === "first" ? nodes[0] : nodes[nodes.length - 1]);
  }

  function collapseOrParent(side) {
    const el = selected[side];
    if (!el) return;
    if (el.tagName === "DETAILS" && el.open) {
      el.open = false;
      return;
    }
    const parent = el.parentElement ? el.parentElement.closest(".node") : null;
    if (parent) select(side, parent);
  }

  function expandOrChild(side) {
    const el = selected[side];
    if (!el) return;
    if (el.tagName === "DETAILS") {
      if (!el.open) {
        el.open = true;
        return;
      }
      const firstChild = el.querySelector(":scope > .node");
      if (firstChild) select(side, firstChild);
    }
  }

  function alignOtherPanel() {
    const el = selected[focusedSide];
    if (!el || !el.dataset.match) {
      setStatus("No mapped counterpart for this node (deleted/inserted)");
      return;
    }
    const other = otherSide(focusedSide);
    const match = document.getElementById(el.dataset.match);
    if (match) select(other, match, { setFocus: false, scrollCounterpart: false });
  }

  function runSearch(side, query) {
    if (!query) return;
    searchQuery[side] = query;
    const nodes = visibleNodes(side);
    if (nodes.length === 0) return;
    let start = nodes.indexOf(selected[side]);
    if (start === -1) start = 0;
    for (let step = 1; step <= nodes.length; step++) {
      const idx = (start + step) % nodes.length;
      const node = nodes[idx];
      if (node.textContent.toLowerCase().includes(query.toLowerCase())) {
        select(side, node);
        setStatus(`Found "${query}"`);
        return;
      }
    }
    setStatus(`No node containing "${query}" found in this panel`);
  }

  function toggleHelp() {
    helpOverlay.classList.toggle("hidden");
  }

  // If hiding identical matches (see style.css's `body.hide-identical` rule) just made either
  // panel's selected node disappear, fall back to that panel's first still-visible node rather
  // than leaving a highlight on an invisible element and j/k stuck with nowhere valid to move
  // from (`moveCursor` looks up the selected node's index in `visibleNodes`).
  function reselectIfHidden() {
    for (const side of SIDES) {
      const el = selected[side];
      if (el && el.getClientRects().length === 0) {
        const nodes = visibleNodes(side);
        if (nodes.length > 0) select(side, nodes[0], { scroll: false });
      }
    }
  }

  function setHideIdentical(hidden) {
    document.body.classList.toggle("hide-identical", hidden);
    toggleIdenticalButton.setAttribute("aria-pressed", hidden ? "true" : "false");
    toggleIdenticalButton.textContent = hidden
      ? "Show identical matches"
      : "Hide identical matches";
    reselectIfHidden();
    setStatus(
      hidden ? "Showing only inserted/deleted/updated nodes and their ancestors" : ""
    );
  }

  function toggleHideIdentical() {
    setHideIdentical(!document.body.classList.contains("hide-identical"));
  }

  function promptSearch() {
    const prompt = document.getElementById("search-prompt");
    const input = document.getElementById("search-input");
    prompt.classList.remove("hidden");
    input.value = searchQuery[focusedSide] || "";
    input.focus();
    input.select();
  }

  function closeSearchPrompt() {
    document.getElementById("search-prompt").classList.add("hidden");
  }

  // Click-to-select: a click anywhere on a node's own label (not its descendants, which are
  // separate .node elements with their own listeners) selects it. `<summary>` already toggles its
  // parent `<details>` natively - this only adds selection/highlighting on top of that.
  for (const side of SIDES) {
    const panel = panels[side];
    if (!panel) continue;
    panel.addEventListener("click", (event) => {
      const target = event.target.closest(".node.leaf, summary");
      if (!target || !panel.contains(target)) return;
      const node = target.classList.contains("node") ? target : target.parentElement;
      select(side, node);
    });
  }

  toggleIdenticalButton.addEventListener("click", toggleHideIdentical);

  document.addEventListener("keydown", (event) => {
    const searchPrompt = document.getElementById("search-prompt");
    if (!searchPrompt.classList.contains("hidden")) {
      if (event.key === "Enter") {
        runSearch(focusedSide, document.getElementById("search-input").value.trim());
        closeSearchPrompt();
        event.preventDefault();
      } else if (event.key === "Escape") {
        closeSearchPrompt();
        event.preventDefault();
      }
      return;
    }

    switch (event.key) {
      case "j":
      case "ArrowDown":
        moveCursor(focusedSide, 1);
        break;
      case "k":
      case "ArrowUp":
        moveCursor(focusedSide, -1);
        break;
      case "h":
      case "ArrowLeft":
        collapseOrParent(focusedSide);
        break;
      case "l":
      case "ArrowRight":
        expandOrChild(focusedSide);
        break;
      case "g":
        jumpEdge(focusedSide, "first");
        break;
      case "G":
        jumpEdge(focusedSide, "last");
        break;
      case "Tab":
        setFocusedSide(otherSide(focusedSide));
        event.preventDefault();
        break;
      case "a":
        alignOtherPanel();
        break;
      case "i":
        toggleHideIdentical();
        break;
      case "/":
        promptSearch();
        event.preventDefault();
        break;
      case "?":
        toggleHelp();
        break;
      case "Escape":
        helpOverlay.classList.add("hidden");
        break;
      default:
        return;
    }
  });

  setFocusedSide("before");
})();
