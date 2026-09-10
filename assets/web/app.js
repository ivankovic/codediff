// The page: DOM, keyboard, mouse and the JSON API. Everything that decides *what* the cursor
// and panels do is model.js; this file measures, draws and fetches. Mirrors src/tui/app.rs
// screen for screen - the same dialogs open on the same keys, and the diff itself only ever
// comes from the server, which computes it with the library the TUI uses.
"use strict";

(() => {
  const M = CodeDiffModel;
  const TOKEN = document.body.dataset.token;
  const TOKEN_HEADER = "X-Codediff-Token";
  const $ = (selector) => document.querySelector(selector);

  // ----- server -------------------------------------------------------------------------------

  async function api(path, body, signal) {
    const response = await fetch(path, {
      method: "POST",
      headers: { "Content-Type": "application/json", [TOKEN_HEADER]: TOKEN },
      body: JSON.stringify(body || {}),
      signal,
    });
    let json = null;
    try {
      json = await response.json();
    } catch (_) {
      json = { error: `${response.status} ${response.statusText}` };
    }
    if (!response.ok || (json && typeof json === "object" && json.error)) {
      throw new Error((json && json.error) || `${response.status} ${response.statusText}`);
    }
    return json;
  }

  // ----- state --------------------------------------------------------------------------------

  const state = {
    info: null, // the /api/state payload
    model: new M.DiffModel(),
    screen: "viewer", // viewer | diffing | editing | quit
    dialog: null,
    themeId: "Dracula",
    palette: null,
    syntaxTheme: null,
    syntax: { background: "#1e1e1e", foreground: "#d4d4d4" },
    syntaxOn: true,
    nodeHighlight: false,
    renderOptions: null,
    presets: null,
    before: null,
    after: null,
    diff: null,
    summary: null,
    counts: null,
    plainText: false,
    lastError: null,
    toast: null,
    toastTimer: null,
    lastSearchQuery: null,
    diffing: null, // { controller, startedAt, timer }
    restoreAfterReload: null,
    recentPairs: [],
    cellWidth: 8,
    rowHeight: 18,
    syncingScroll: false,
  };

  const COLOR_SLOTS = [
    ["insert_bg", "Insert"],
    ["delete_bg", "Delete"],
    ["update_bg", "Update"],
    ["move_bg", "Move"],
    ["cross_highlight_bg", "Cursor counterpart"],
    ["search_bg", "Search match"],
    ["overlay_fg", "Overlay text"],
    ["before_title_fg", '"Before" title'],
    ["after_title_fg", '"After" title'],
  ];

  const SUMMARY_COLORS = {
    no_changes: "#7f7f7f",
    new_file: "#00cd00",
    deleted_file: "#cd0000",
    whitespace_only: "#00cdcd",
    comment_only: "#5c5cff",
    refactor_moved_only: "#cdcd00",
  };

  function themeById(id) {
    return state.info.themes.find((theme) => theme.id === id) || state.info.themes[0];
  }

  function paletteFor(id) {
    return { ...themeById(id).palette };
  }

  function setError(message) {
    state.lastError = message;
  }

  // ----- measurement ----------------------------------------------------------------------------

  function measure() {
    const probe = $("#measure");
    state.cellWidth = probe.getBoundingClientRect().width / probe.textContent.length;
    state.rowHeight = parseFloat(getComputedStyle(probe).lineHeight) || 18;
  }

  function panelElements(index) {
    const root = document.getElementById(`panel-${index}`);
    return { root, code: root.querySelector(".code"), rows: root.querySelector(".rows"), minimap: root.querySelector(".minimap") };
  }

  function gutterCells(panel) {
    return panel.lineCount() === 0 ? 0 : String(panel.lineCount()).length + 1;
  }

  function updateGeometry() {
    const panelsBox = $("#panels");
    const widthCells = Math.floor(panelsBox.clientWidth / state.cellWidth);
    state.model.updateDisplayMode(widthCells, state.info.single_panel_threshold);
    panelsBox.classList.toggle("single", state.model.displayMode === "single");
    for (let i = 0; i < 2; i++) {
      const panel = state.model.panels[i];
      const { root, code } = panelElements(i);
      const visible = state.model.displayMode === "dual" || state.model.activePanel === i;
      root.hidden = !visible;
      if (!visible) continue;
      panel.viewportHeight = Math.max(Math.floor(code.clientHeight / state.rowHeight), 1);
      const textWidth = code.clientWidth - gutterCells(panel) * state.cellWidth;
      panel.viewportWidth = Math.max(Math.floor(textWidth / state.cellWidth), 1);
    }
  }

  // ----- rendering ------------------------------------------------------------------------------

  function render() {
    updateGeometry();
    document.documentElement.style.setProperty("--page-bg", state.syntax.background);
    document.documentElement.style.setProperty("--page-fg", state.syntax.foreground);
    renderTitles();
    for (let i = 0; i < 2; i++) renderPanel(i);
    renderStatus();
    renderFooter();
    renderOverlays();
  }

  function renderTitles() {
    const names = ["Before", "After"];
    for (let i = 0; i < 2; i++) {
      const panel = state.model.panels[i];
      const { root } = panelElements(i);
      const title = root.querySelector(".title");
      const active = state.model.activePanel === i;
      const color = i === 0 ? state.palette.before_title_fg : state.palette.after_title_fg;
      const label = title.querySelector(".label");
      label.textContent = ` ${names[i]} `;
      label.style.color = active ? "#000" : color;
      label.style.background = active ? color : "transparent";
      const file = title.querySelector(".file");
      const name = panel.hasFile() ? panel.name : "(press 'o' to open a file)";
      if (state.model.displayMode === "single") {
        file.innerHTML = "";
        file.append(" - ");
        const strong = document.createElement("span");
        strong.className = "filename";
        strong.textContent = name;
        file.append(strong, " - ");
        const language = document.createElement("span");
        language.className = "language";
        language.textContent = panel.language;
        file.append(language, " (Tab to switch)");
      } else {
        file.textContent = ` ${name}`;
      }
    }
  }

  function renderPanel(index) {
    const panel = state.model.panels[index];
    const { root, code, rows, minimap } = panelElements(index);
    if (root.hidden) return;
    const total = panel.lineCount();
    rows.style.height = `${Math.max(total, 1) * state.rowHeight}px`;
    const gutter = gutterCells(panel);
    rows.style.setProperty("--gutter-cells", gutter);

    // Keep the native scroll position in step with the model without re-entering the scroll
    // handler's own model update.
    state.syncingScroll = true;
    code.scrollTop = panel.scroll * state.rowHeight;
    code.scrollLeft = panel.scrollCol * state.cellWidth;
    state.syncingScroll = false;

    const first = Math.max(panel.scroll - 2, 0);
    const last = Math.min(panel.scroll + panel.viewportHeight + 2, total);
    const fragment = document.createDocumentFragment();
    for (let row = first; row < last; row++) {
      fragment.append(renderRow(panel, row, gutter));
    }
    rows.replaceChildren(fragment);
    renderMinimap(panel, code, minimap);
  }

  function renderRow(panel, row, gutter) {
    const text = panel.lineText(row);
    const paints = panel.rowPaints(row, state.palette, state.nodeHighlight);
    const cursorHere = panel.focused && row === panel.cursorRow;
    if (cursorHere && panel.cursorCol < text.length) {
      paints.push({ start: panel.cursorCol, end: M.stepRight(text, panel.cursorCol), cursor: true });
    }
    const spans = state.syntaxOn ? panel.spans[row] || [] : [];
    const element = document.createElement("div");
    element.className = "row";
    element.dataset.row = row;
    element.style.top = `${row * state.rowHeight}px`;
    const gutterElement = document.createElement("span");
    gutterElement.className = "gutter";
    gutterElement.textContent = String(row + 1).padStart(gutter - 1, " ") + " ";
    element.append(gutterElement);
    const textElement = document.createElement("span");
    textElement.className = "text";
    for (const segment of M.rowSegments(text, spans, paints)) {
      const piece = document.createElement("span");
      piece.textContent = text.slice(segment.start, segment.end);
      if (segment.cursor) piece.className = "cursor";
      if (segment.fg) piece.style.color = segment.fg;
      if (segment.bg) piece.style.background = segment.bg;
      textElement.append(piece);
    }
    if (cursorHere && panel.cursorCol >= text.length) {
      const eol = document.createElement("span");
      eol.className = "cursor eol";
      eol.textContent = " ";
      textElement.append(eol);
    }
    element.append(textElement);
    return element;
  }

  function renderMinimap(panel, code, canvas) {
    const bands = panel.viewportHeight;
    const height = code.clientHeight;
    canvas.hidden = panel.lineCount() === 0;
    if (canvas.hidden) return;
    canvas.width = 8;
    canvas.height = height;
    const context = canvas.getContext("2d");
    context.clearRect(0, 0, canvas.width, canvas.height);
    const bandHeight = height / Math.max(bands, 1);
    panel.changeBands(bands).forEach((op, band) => {
      const color = op && M.backgroundFor(op, state.palette);
      if (!color) return;
      context.fillStyle = color;
      context.fillRect(0, band * bandHeight, canvas.width, Math.max(bandHeight, 1));
    });
  }

  function renderStatus() {
    const status = $("#status");
    status.hidden = !state.summary;
    if (state.summary) {
      status.textContent = state.summary.label;
      status.style.color = SUMMARY_COLORS[state.summary.kind] || "";
    }
    const error = $("#error");
    error.hidden = !state.lastError;
    error.textContent = state.lastError || "";
  }

  function renderFooter() {
    const model = state.model;
    $("#footer-left").textContent = M.footerLeft({
      cursor: model.focusedCursorPosition(),
      counts: state.counts,
      searchProgress: model.focusedSearchMatchCountAndIndex(),
      changeProgress: model.mergedChangeCountAndIndex(),
      plainText: state.plainText,
      layout: model.layoutOverride,
      options: state.renderOptions,
      rows: state.info.render_option_rows,
      presets: state.presets,
    });
    $("#footer-right").textContent = M.FOOTER_HINTS;
  }

  function renderOverlays() {
    const diffing = $("#diffing");
    diffing.hidden = state.screen !== "diffing";
    if (state.diffing) {
      const elapsed = ((Date.now() - state.diffing.startedAt) / 1000).toFixed(1);
      diffing.textContent = `Diffing… ${elapsed}s (Esc cancels)`;
    }
    $("#editing").hidden = state.screen !== "editing";
    $("#quit").hidden = state.screen !== "quit";

    const toast = $("#toast");
    toast.hidden = !state.toast;
    if (state.toast) {
      toast.textContent = state.toast.label;
      toast.style.color = SUMMARY_COLORS[state.toast.kind] || "";
      toast.style.borderColor = SUMMARY_COLORS[state.toast.kind] || "";
    }

    const recent = $("#recent");
    const showRecent =
      state.screen === "viewer" && !state.dialog && !state.before && !state.after && state.recentPairs.length > 0;
    recent.hidden = !showRecent;
    if (showRecent) {
      const lines = ["Recent pairs"];
      state.recentPairs.slice(0, 9).forEach(([before, after], i) => {
        lines.push(`  ${i + 1}  ${before}  ↔  ${after}`);
      });
      lines.push("  press a digit to reopen, or 'o' to browse");
      recent.textContent = lines.join("\n");
    }

    const overlay = $("#overlay");
    overlay.hidden = !state.dialog;
    if (state.dialog) state.dialog.render(overlay);
    document.body.classList.toggle("file-dialog", !!state.dialog && state.dialog.fullScreen === true);
  }

  // ----- diffing --------------------------------------------------------------------------------

  function rememberCursorForRestore() {
    const cursor = state.model.focusedCursorPosition();
    if (cursor) state.restoreAfterReload = { panel: state.model.activePanel, row: cursor[0], col: cursor[1] };
  }

  async function startDiff(before, after) {
    cancelInFlight();
    state.before = before;
    state.after = after;
    const controller = new AbortController();
    state.diffing = { controller, startedAt: Date.now(), timer: setInterval(renderOverlays, 100) };
    state.screen = "diffing";
    state.summary = null;
    state.counts = null;
    state.plainText = false;
    render();
    try {
      const payload = await api("/api/diff", { before, after }, controller.signal);
      applyDiff(payload);
    } catch (error) {
      if (error.name === "AbortError") return;
      setError(error.message);
      state.restoreAfterReload = null;
    } finally {
      if (state.diffing && state.diffing.controller === controller) {
        clearInterval(state.diffing.timer);
        state.diffing = null;
        if (state.screen === "diffing") state.screen = "viewer";
      }
      render();
    }
  }

  function cancelInFlight() {
    if (!state.diffing) return;
    clearInterval(state.diffing.timer);
    state.diffing.controller.abort();
    state.diffing = null;
  }

  // Esc while diffing: drop the answer (the server retires the generation too) and keep what was
  // shown before, exactly as the TUI does.
  function cancelDiff() {
    cancelInFlight();
    state.restoreAfterReload = null;
    state.screen = "viewer";
    api("/api/cancel").catch(() => {});
    render();
  }

  function applyDiff(payload) {
    state.diff = payload;
    state.model.loadDiff(payload);
    if (state.restoreAfterReload) {
      const { panel, row, col } = state.restoreAfterReload;
      state.model.restoreCursor(panel, row, col);
      state.restoreAfterReload = null;
    }
    state.before = payload.before.path;
    state.after = payload.after.path;
    state.summary = payload.summary;
    state.counts = payload.change_counts;
    state.plainText = payload.plain_text_fallback;
    state.renderOptions = payload.render_options;
    state.syntax = payload.syntax;
    state.lastError = null;
    const pair = [payload.before.path, payload.after.path];
    state.recentPairs = [pair, ...state.recentPairs.filter((p) => p[0] !== pair[0] || p[1] !== pair[1])];
    showToast(payload.summary);
    state.screen = "viewer";
  }

  function showToast(summary) {
    clearTimeout(state.toastTimer);
    state.toast = summary || null;
    if (summary) state.toastTimer = setTimeout(() => dismissToast(true), 4000);
  }

  function dismissToast(rerender) {
    if (!state.toast) return;
    clearTimeout(state.toastTimer);
    state.toast = null;
    if (rerender) renderOverlays();
  }

  function reload() {
    if (!state.before || !state.after) return;
    rememberCursorForRestore();
    startDiff(state.before, state.after);
  }

  async function applyRenderOptions(options) {
    state.renderOptions = { ...options };
    try {
      const payload = await api("/api/render_options", options);
      if (payload) {
        state.model.replaceRanges(payload.before.ranges, payload.after.ranges);
        state.summary = payload.summary;
        state.counts = payload.change_counts;
        state.renderOptions = payload.render_options;
      }
    } catch (error) {
      setError(error.message);
    }
    render();
  }

  async function edit() {
    const cursor = state.model.focusedCursorPosition();
    const panel = state.model.activePanel === 0 ? "before" : "after";
    if (!cursor || !(panel === "before" ? state.before : state.after)) return;
    state.screen = "editing";
    render();
    try {
      await api("/api/edit", { panel, line: cursor[0] + 1 });
    } catch (error) {
      setError(error.message);
    }
    state.screen = "viewer";
    reload();
  }

  async function quit() {
    state.screen = "quit";
    state.dialog = null;
    render();
    try {
      await api("/api/quit");
    } catch (_) {
      // The server closes as it answers; a failed read here is the expected outcome.
    }
  }

  function saveSettings(update) {
    api("/api/settings", update).catch((error) => {
      setError(error.message);
      renderStatus();
    });
  }

  // ----- dialogs --------------------------------------------------------------------------------

  function closeDialog() {
    state.dialog = null;
    render();
  }

  function box(container, title, hint) {
    container.innerHTML = "";
    const dialog = document.createElement("div");
    dialog.className = "dialog";
    const heading = document.createElement("div");
    heading.className = "dialog-title";
    heading.textContent = title;
    const body = document.createElement("div");
    body.className = "dialog-body";
    const footer = document.createElement("div");
    footer.className = "dialog-hint";
    footer.textContent = hint;
    dialog.append(heading, body, footer);
    container.append(dialog);
    return { dialog, body, heading };
  }

  function listRows(body, labels, selected) {
    labels.forEach((label, index) => {
      const row = document.createElement("div");
      row.className = "list-row" + (index === selected ? " selected" : "");
      row.textContent = label;
      body.append(row);
      if (index === selected) row.scrollIntoView({ block: "nearest" });
    });
  }

  function moveSelection(selected, delta, length) {
    if (delta < 0) return Math.max(selected - 1, 0);
    return selected + 1 < length ? selected + 1 : selected;
  }

  function isPrintable(e) {
    return e.key.length === 1 && !e.ctrlKey && !e.metaKey && !e.altKey;
  }

  // `o`: FileDialog. Full-screen, as in the TUI.
  function fileDialog(panelIndex) {
    const dialog = {
      fullScreen: true,
      title: panelIndex === 0 ? "Select the BEFORE file" : "Select the AFTER file",
      dir: "",
      entries: [],
      selected: 0,
      filter: "",
      showHidden: false,
      visible() {
        const filter = this.filter.toLowerCase();
        return this.entries.filter((entry) => {
          if (entry.name === "..") return true;
          if (!this.showHidden && entry.name.startsWith(".")) return false;
          return !filter || entry.name.toLowerCase().includes(filter);
        });
      },
      async list(path) {
        try {
          const listing = await api("/api/ls", { path });
          if (state.dialog !== this) return;
          this.dir = listing.dir;
          this.entries = listing.entries;
          this.selected = 0;
          this.filter = "";
        } catch (error) {
          setError(error.message);
        }
        render();
      },
      key(e) {
        const visible = this.visible();
        if (e.key === "ArrowUp") this.selected = moveSelection(this.selected, -1, visible.length);
        else if (e.key === "ArrowDown") this.selected = moveSelection(this.selected, 1, visible.length);
        else if (e.key === "Enter") {
          const entry = visible[this.selected];
          if (!entry) return;
          if (entry.is_dir) this.list(entry.path);
          else {
            state.dialog = null;
            selectFileForPanel(panelIndex, entry.path);
          }
          return;
        } else if (e.key === "h" && e.ctrlKey) {
          this.showHidden = !this.showHidden;
          this.selected = 0;
        } else if (e.key === "Backspace") {
          if (this.filter) this.filter = this.filter.slice(0, -1);
          else {
            const parent = this.entries.find((entry) => entry.name === "..");
            if (parent) this.list(parent.path);
            return;
          }
          this.selected = 0;
        } else if (e.key === "Escape") {
          closeDialog();
          return;
        } else if (isPrintable(e)) {
          this.filter += e.key;
          this.selected = 0;
        }
        render();
      },
      render(container) {
        const title = `${this.title} - ${this.dir}${this.filter ? `  filter: ${this.filter}` : ""}`;
        const { body } = box(
          container,
          title,
          "type: filter | Enter: select/open dir | Backspace: unfilter/parent | Ctrl-h: hidden | Esc: cancel"
        );
        listRows(body, this.visible().map((entry) => (entry.is_dir ? `${entry.name}/` : entry.name)), this.selected);
      },
    };
    dialog.list(null);
    return dialog;
  }

  function selectFileForPanel(panelIndex, path) {
    if (panelIndex === 0) state.before = path;
    else state.after = path;
    state.lastError = null;
    if (state.before && state.after) startDiff(state.before, state.after);
    else render();
  }

  // `c`: ThemeDialog.
  function themeDialog() {
    const themes = state.info.themes;
    const syntaxThemes = state.info.syntax_themes;
    const savedSyntax = state.syntaxTheme;
    const dialog = {
      themeIndex: Math.max(themes.findIndex((theme) => theme.id === state.themeId), 0),
      syntaxIndex: Math.max(syntaxThemes.indexOf(state.syntaxTheme), 0),
      working: paletteFor(state.themeId),
      selected: 0,
      editing: null,
      rowCount: 2 + COLOR_SLOTS.length,
      theme() {
        return themes[this.themeIndex];
      },
      async previewSyntax(name) {
        try {
          const payload = await api("/api/highlight", { syntax_theme: name });
          if (payload) {
            state.model.panels[0].spans = payload.before;
            state.model.panels[1].spans = payload.after;
            state.syntax = payload.syntax;
          }
        } catch (error) {
          setError(error.message);
        }
        render();
      },
      key(e) {
        if (this.editing !== null) {
          if (isPrintable(e) && /[0-9a-fA-F#]/.test(e.key)) {
            if (this.editing.replace(/^#+/, "").length < 6 || e.key === "#") this.editing += e.key;
          } else if (e.key === "Backspace") this.editing = this.editing.slice(0, -1);
          else if (e.key === "Enter") {
            const typed = this.editing;
            this.editing = null;
            const slot = COLOR_SLOTS[this.selected - 2];
            if (slot && /^#?[0-9a-fA-F]{6}$/.test(typed.trim())) {
              this.working[slot[0]] = "#" + typed.trim().replace(/^#/, "").toLowerCase();
              this.themeIndex = themes.findIndex((theme) => theme.id === "Custom");
              state.palette = { ...this.working };
            }
          } else if (e.key === "Escape") this.editing = null;
          render();
          return;
        }
        if (e.key === "ArrowUp") this.selected = moveSelection(this.selected, -1, this.rowCount);
        else if (e.key === "ArrowDown") this.selected = moveSelection(this.selected, 1, this.rowCount);
        else if (e.key === "ArrowLeft" || e.key === "ArrowRight") {
          const delta = e.key === "ArrowLeft" ? -1 : 1;
          if (this.selected === 0) {
            this.themeIndex = moveSelection(this.themeIndex, delta, themes.length);
            this.working = { ...this.theme().palette };
            state.palette = { ...this.working };
          } else if (this.selected === 1 && syntaxThemes.length) {
            this.syntaxIndex = moveSelection(this.syntaxIndex, delta, syntaxThemes.length);
            this.previewSyntax(syntaxThemes[this.syntaxIndex]);
            return;
          }
        } else if (e.key === "Enter") {
          if (this.selected >= 2) this.editing = this.working[COLOR_SLOTS[this.selected - 2][0]];
          else {
            this.commit();
            return;
          }
        } else if (e.key === "Escape") {
          this.cancel();
          return;
        }
        render();
      },
      commit() {
        const custom = themes.find((theme) => theme.id === "Custom");
        if (custom) custom.palette = { ...this.working };
        const syntaxName = syntaxThemes[this.syntaxIndex];
        state.themeId = this.theme().id;
        state.palette = { ...this.working };
        state.syntaxTheme = syntaxName || state.syntaxTheme;
        saveSettings({ theme: state.themeId, custom_palette: this.working, syntax_theme: syntaxName });
        closeDialog();
      },
      cancel() {
        state.palette = paletteFor(state.themeId);
        const previewed = syntaxThemes[this.syntaxIndex];
        state.dialog = null;
        if (previewed !== savedSyntax && state.diff) this.previewSyntax(savedSyntax || "base16-ocean.dark");
        else render();
      },
      render(container) {
        const hint = this.editing !== null ? "type #rrggbb | Enter: apply | Esc: cancel edit" : "←/→: change | Enter: edit or accept | Esc: cancel";
        const { body } = box(container, "Theme", hint);
        const row = (index, label, valueNode) => {
          const element = document.createElement("div");
          element.className = "list-row theme-row" + (index === this.selected ? " selected" : "");
          const marker = document.createElement("span");
          marker.textContent = index === this.selected ? "> " : "  ";
          const name = document.createElement("span");
          name.className = "theme-label";
          name.textContent = label;
          element.append(marker, name, valueNode);
          body.append(element);
        };
        const dropdown = (text) => {
          const span = document.createElement("span");
          span.className = "dropdown";
          span.textContent = `< ${text} >`;
          return span;
        };
        row(0, "Theme:", dropdown(this.theme().label));
        row(1, "Syntax theme:", dropdown(syntaxThemes[this.syntaxIndex] || "(none)"));
        COLOR_SLOTS.forEach(([key, label], i) => {
          const index = 2 + i;
          const stored = this.working[key];
          const shown = this.editing !== null && index === this.selected ? this.editing : stored;
          const value = document.createElement("span");
          const swatch = document.createElement("span");
          swatch.className = "swatch";
          swatch.style.background = /^#?[0-9a-fA-F]{6}$/.test(shown.trim()) ? "#" + shown.trim().replace(/^#/, "") : stored;
          const text = document.createElement("span");
          text.textContent = "  " + shown + (this.editing !== null && index === this.selected ? "_" : "");
          value.append(swatch, text);
          row(index, label, value);
        });
      },
    };
    return dialog;
  }

  // `M`: RenderOptionsDialog.
  function renderOptionsDialog() {
    const rows = state.info.render_option_rows;
    const dialog = {
      options: { ...state.renderOptions },
      initial: { ...state.renderOptions },
      selected: 0,
      key(e) {
        if (e.key === "ArrowUp") this.selected = moveSelection(this.selected, -1, rows.length);
        else if (e.key === "ArrowDown") this.selected = moveSelection(this.selected, 1, rows.length);
        else if (e.key === "Enter" || e.key === " ") {
          const key = rows[this.selected].key;
          this.options[key] = !this.options[key];
          applyRenderOptions(this.options);
          return;
        } else if (e.key === "1") {
          this.options = { ...state.presets.minimal };
          applyRenderOptions(this.options);
          return;
        } else if (e.key === "2") {
          this.options = { ...state.presets.full };
          applyRenderOptions(this.options);
          return;
        } else if (e.key === "Escape") {
          const differs = rows.some((row) => this.options[row.key] !== this.initial[row.key]);
          state.dialog = null;
          if (differs) applyRenderOptions(this.initial);
          else render();
          return;
        }
        render();
      },
      render(container) {
        const { body } = box(container, "Render options", "↑/↓ move  Enter/Space toggle  1: minimal  2: full  Esc: cancel");
        listRows(body, rows.map((row) => `[${this.options[row.key] ? "x" : " "}] ${row.label}`), this.selected);
      },
    };
    return dialog;
  }

  // `?`: HelpModal.
  function helpModal() {
    return {
      scroll: 0,
      key(e) {
        if (e.key === "?" || e.key === "Escape") {
          closeDialog();
          return;
        }
        if (e.key === "j" || e.key === "ArrowDown") this.scroll += 1;
        else if (e.key === "k" || e.key === "ArrowUp") this.scroll = Math.max(this.scroll - 1, 0);
        render();
      },
      render(container) {
        const { dialog, body } = box(container, "Help - j/k scroll, ? or Esc to close", "");
        dialog.classList.add("help");
        const legend = document.createElement("div");
        legend.className = "legend";
        legend.append("Colors (current theme)\n  ");
        for (const [label, bg] of [
          ["inserted", state.palette.insert_bg],
          ["deleted", state.palette.delete_bg],
          ["moved", state.palette.move_bg],
          ["updated", state.palette.update_bg],
          ["cursor/counterpart", state.palette.cross_highlight_bg],
          ["search match", state.palette.search_bg],
        ]) {
          const swatch = document.createElement("span");
          swatch.textContent = ` ${label} `;
          swatch.style.background = bg;
          swatch.style.color = state.palette.overlay_fg;
          legend.append(swatch, " ");
        }
        legend.append("\n\n");
        const text = document.createElement("pre");
        text.textContent = state.info.help_text;
        body.append(legend, text);
        body.scrollTop = this.scroll * state.rowHeight;
      },
    };
  }

  // `/`: SearchModal.
  function searchModal() {
    return {
      query: "",
      count: null,
      key(e) {
        if (e.key === "Enter") {
          state.dialog = null;
          submitSearch(this.query);
          return;
        }
        if (e.key === "Escape") {
          state.dialog = null;
          state.model.previewSearch(state.lastSearchQuery || "");
          render();
          return;
        }
        if (e.key === "Backspace") this.query = this.query.slice(0, -1);
        else if (isPrintable(e)) this.query += e.key;
        else return;
        this.count = state.model.previewSearch(this.query);
        render();
      },
      render(container) {
        const title = this.count === null ? "Search" : this.count === 1 ? "Search - 1 match" : `Search - ${this.count} matches`;
        const hint =
          state.lastSearchQuery && !this.query ? `Enter: repeat '${state.lastSearchQuery}' | Esc: cancel` : "Enter: jump to match | Esc: cancel";
        const { body } = box(container, title, hint);
        const input = document.createElement("div");
        input.className = "prompt";
        input.textContent = `/${this.query}`;
        const caret = document.createElement("span");
        caret.className = "cursor eol";
        caret.textContent = " ";
        input.append(caret);
        body.append(input);
      },
    };
  }

  function submitSearch(query) {
    const effective = query || state.lastSearchQuery || "";
    if (effective) state.lastSearchQuery = effective;
    state.model.search(effective);
    render();
  }

  // `g`: LinePrompt.
  function linePrompt() {
    return {
      digits: "",
      key(e) {
        if (e.key === "Escape") {
          closeDialog();
          return;
        }
        if (e.key === "Enter") {
          const line = parseInt(this.digits, 10);
          state.dialog = null;
          if (line > 0) state.model.jumpToLine(line);
          render();
          return;
        }
        if (e.key === "Backspace") this.digits = this.digits.slice(0, -1);
        else if (/^[0-9]$/.test(e.key) && isPrintable(e)) this.digits += e.key;
        else return;
        render();
      },
      render(container) {
        const { body } = box(container, "Go to line", "Enter: jump | Esc: cancel");
        const input = document.createElement("div");
        input.className = "prompt";
        input.textContent = `:${this.digits}`;
        const caret = document.createElement("span");
        caret.className = "cursor eol";
        caret.textContent = " ";
        input.append(caret);
        body.append(input);
      },
    };
  }

  // ----- keyboard -------------------------------------------------------------------------------

  function onKeyDown(e) {
    if (state.screen === "quit" || state.screen === "editing") return;
    if (e.metaKey || e.altKey) return;
    dismissToast(true);
    if (state.screen === "diffing") {
      if (e.key === "Escape") cancelDiff();
      else if (e.key === "q") quit();
      e.preventDefault();
      return;
    }
    if (state.dialog) {
      if (e.key === "Tab") e.preventDefault();
      state.dialog.key(e);
      e.preventDefault();
      return;
    }
    if (viewerKey(e)) e.preventDefault();
  }

  // The viewer's bindings: `App::handle_events`' global keys first, then `DiffViewer`'s.
  function viewerKey(e) {
    const model = state.model;
    const key = e.key;
    const noFiles = !state.before && !state.after;
    if (e.ctrlKey) {
      if (key === "d") model.moveCursorHalfPage(1);
      else if (key === "u") model.moveCursorHalfPage(-1);
      else if (key === "e") model.scrollView(1);
      else if (key === "y") model.scrollView(-1);
      else return false;
      render();
      return true;
    }
    switch (key) {
      case "q":
      case "Escape":
        quit();
        return true;
      case "r":
        reload();
        return true;
      case "g":
        state.dialog = linePrompt();
        break;
      case "e":
        edit();
        return true;
      case "o":
        state.dialog = fileDialog(model.activePanel);
        break;
      case "c":
        state.dialog = themeDialog();
        break;
      case "M":
        if (state.renderOptions) state.dialog = renderOptionsDialog();
        break;
      case "?":
        state.dialog = helpModal();
        break;
      case "/":
        state.dialog = searchModal();
        break;
      case "Enter":
        model.jumpToCounterpart();
        break;
      case "v":
        model.cycleLayoutOverride();
        saveSettings({ layout: model.layoutOverride });
        break;
      case "S":
        state.syntaxOn = !state.syntaxOn;
        break;
      case "H":
        state.nodeHighlight = !state.nodeHighlight;
        saveSettings({ node_highlight: state.nodeHighlight });
        break;
      case "Tab":
        model.toggleActivePanel();
        model.syncCrossHighlight();
        break;
      case "ArrowUp":
      case "k":
        model.moveCursorVertical(-1);
        break;
      case "ArrowDown":
      case "j":
        model.moveCursorVertical(1);
        break;
      case "ArrowLeft":
      case "h":
        model.moveCursorHorizontal(-1);
        break;
      case "ArrowRight":
      case "l":
        model.moveCursorHorizontal(1);
        break;
      case "n":
        model.jumpToChange(true);
        break;
      case "p":
        model.jumpToChange(false);
        break;
      case ">":
        model.jumpToSearchMatch(true);
        break;
      case "<":
        model.jumpToSearchMatch(false);
        break;
      case "PageUp":
        model.pageScroll(-1);
        break;
      case "PageDown":
        model.pageScroll(1);
        break;
      case "Home":
        model.home();
        break;
      case "End":
        model.end();
        break;
      default:
        if (/^[1-9]$/.test(key) && noFiles) {
          const pair = state.recentPairs[Number(key) - 1];
          if (pair) startDiff(pair[0], pair[1]);
          return true;
        }
        return false;
    }
    render();
    return true;
  }

  // ----- mouse ----------------------------------------------------------------------------------

  function columnFromPoint(textElement, x, y) {
    const position = document.caretPositionFromPoint
      ? document.caretPositionFromPoint(x, y)
      : document.caretRangeFromPoint
        ? (() => {
            const range = document.caretRangeFromPoint(x, y);
            return range && { offsetNode: range.startContainer, offset: range.startOffset };
          })()
        : null;
    if (position && textElement.contains(position.offsetNode)) {
      let column = position.offset;
      let node = position.offsetNode.nodeType === Node.TEXT_NODE ? position.offsetNode.parentNode : position.offsetNode;
      while (node && node !== textElement) {
        let sibling = node.previousSibling;
        while (sibling) {
          column += sibling.textContent.length;
          sibling = sibling.previousSibling;
        }
        node = node.parentNode;
      }
      return column;
    }
    const rect = textElement.getBoundingClientRect();
    return Math.max(Math.round((x - rect.left) / state.cellWidth), 0);
  }

  function onPanelClick(index, e) {
    if (state.dialog || state.screen !== "viewer") return;
    const rowElement = e.target.closest(".row");
    if (!rowElement) return;
    const row = Number(rowElement.dataset.row);
    const textElement = rowElement.querySelector(".text");
    const col = e.target.closest(".gutter") ? 0 : columnFromPoint(textElement, e.clientX, e.clientY);
    state.model.clickAt(index, row, col);
    render();
  }

  function onPanelScroll(index) {
    if (state.syncingScroll) return;
    const panel = state.model.panels[index];
    const { code } = panelElements(index);
    panel.scroll = Math.round(code.scrollTop / state.rowHeight);
    panel.scrollCol = Math.round(code.scrollLeft / state.cellWidth);
    renderPanel(index);
  }

  // ----- start ----------------------------------------------------------------------------------

  async function init() {
    try {
      state.info = await api("/api/state");
    } catch (error) {
      document.body.textContent = `codediff-web: ${error.message}`;
      return;
    }
    const settings = state.info.settings;
    state.themeId = settings.theme;
    state.palette = paletteFor(settings.theme);
    state.syntaxTheme = settings.syntax_theme;
    state.nodeHighlight = settings.node_highlight;
    state.renderOptions = settings.render_options;
    state.presets = state.info.presets;
    state.model.layoutOverride = settings.layout;
    state.recentPairs = state.info.recent_pairs;
    state.before = state.info.before;
    state.after = state.info.after;
    if (state.info.config_error) setError(state.info.config_error);
    document.title = `codediff ${state.info.version}`;

    measure();
    for (let i = 0; i < 2; i++) {
      const { code } = panelElements(i);
      code.addEventListener("click", (e) => onPanelClick(i, e));
      code.addEventListener("scroll", () => onPanelScroll(i));
    }
    document.addEventListener("keydown", onKeyDown);
    window.addEventListener("resize", () => {
      measure();
      render();
    });
    render();
    if (state.before && state.after) startDiff(state.before, state.after);
  }

  init();
})();
