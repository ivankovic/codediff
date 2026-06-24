# TUI specs

High-level architecture decisions and the rationale behind them. See the root README.md for the
overall Component architecture pattern this follows.

## Async event loop

All terminal input, ticks, and render timers are merged into a single `tokio::select!` loop inside
`UI::next_event` (`ui.rs`), called once per `App::run` iteration. Terminal input specifically comes
from `crossterm::event::EventStream` (an async, epoll-driven stream), not from manual polling on a
dedicated OS thread. A prior version of this code spawned a thread that called
`crossterm::event::poll` with a zero timeout in a tight loop, sleeping a few microseconds when idle;
that burns a full CPU core continuously and was a major source of the TUI feeling slow. There is no
dedicated input thread or task at all: `UI` owns the `EventStream` and the tick/render
`tokio::time::Interval`s directly, and `next_event` loops internally (without returning) past
crossterm event kinds the app has no use for (focus/paste), only ever yielding `None` once the
input stream itself closes.

Tick rate and frame rate remain independently configurable (defaults: 4 Hz tick, 60 fps render);
the builder methods `UI::tick_rate`/`frame_rate` rebuild the corresponding `Interval` so they take
effect even though they're called after `UI::new()`.

`Component::handle_events` takes the TUI's own `tui::events::Event` (`Tick`/`Render`/`Resize`/
`Key`/`Mouse`), not `crossterm::event::Event` directly, since the merged loop needs to fold tick
and render notifications into the same enum as raw input.

## Syntax highlighting is cached, not recomputed per frame

`CodeViewerWidget` highlights the *entire* file once, whenever the file is loaded or the
highlighting theme/toggle changes, and caches the resulting styled lines. Rendering a frame only
slices the cached lines for the visible viewport and applies the diff/cursor overlay (see below).
Previously, syntax highlighting was recomputed for the visible window on every single render frame
(up to 60 times per second per panel), which dominated CPU cost for any real-sized file. Recomputing
the cache is only ever triggered by an explicit content/setting change, never by scrolling or the
render tick.

## Diff overlay and cursor model

`TextDiff::all(side)` returns a `Vec<RangeMatch>`, where each entry's `source` field is a range on
this side and `destination` is the matching range on the other side (this pairing already exists
in the diff/text module; the TUI does not invent any new matching logic). The TUI reuses this
directly:

- The cursor is a literal `(row, column)` position (`CodeViewerState::cursor_row`/`cursor_col` in
  `widgets/code_viewer.rs`), like a normal text cursor, and is rendered as the *real* terminal
  cursor (`Frame::set_cursor`, called from `DiffViewer::draw` for whichever panel is focused) —
  not a synthetic highlight, so it's visible by construction regardless of color scheme. Arrow
  Up/Down and vim `k`/`j` move the row by one line, clamping the column to the new line's length;
  Left/Right and `h`/`l` move the column by one character, wrapping to the previous/next line at
  row boundaries. An earlier version modeled the cursor as an index into the flat, document-ordered
  `Vec<RangeMatch>`, which made `h`/`l` meaningless (mapped the same as `k`/`j`) and the cursor's
  only visual signal a (sometimes hard-to-spot) background color; a real `(row, column)` cursor
  reads like any other editor and the terminal's own cursor rendering is guaranteed visible.
- Mapping `(row, column)` to the `RangeMatch` it falls in (needed for the cross-highlight, below)
  is a binary search, not a scan: `CodeViewerState::load_ranges` builds `range_order`, a `Vec<usize>`
  of indices into `ranges` sorted by source start position (end position as a secondary key, so a
  zero-width alignment marker sharing a start with a real range sorts before it and never wins the
  lookup). `range_at` (`widgets/code_viewer.rs`) then finds the covering range in O(log n) via
  `partition_point`, run once per cursor movement (and once per visible row at render time, to know
  which row needs the blue overlay) rather than once per range.
- The range under the cursor's `(row, column)` recomputes its `destination` and pushes it onto the
  *other* panel as its cross-highlight target. This is the only data needed to highlight "the
  matched node" on the other side: there is no separate node-matching search at the TUI layer.
- Background colors per `TextOperation`: `Insert` green, `Delete` red, `Move` yellow/olive,
  `Update` magenta (dark), `Identical` left as plain syntax highlighting. Both the range under the
  cursor on the focused panel *and* the cross-highlighted matched range on the other panel use the
  same blue background, which takes priority over whatever diff-operation color that range would
  otherwise have, since it is a more specific, momentary signal than the static diff coloring (and
  using the same blue on both sides makes it visually obvious they're the same node). This range
  highlight is in addition to, not instead of, the real terminal cursor described above: the blue
  shows the matched AST node's full extent, while the cursor pinpoints the exact character. The
  blue is deliberately brighter/more saturated than the four diff colors so it doesn't blend in
  with similarly-dark diff backgrounds.
- Every diff/cursor overlay background is paired with an explicit light foreground (`OVERLAY_FG`
  in `widgets/code_viewer.rs`), rather than leaving the foreground at whatever it already was.
  Plain, non-overlaid text relies on the terminal's own default foreground (fine, since it's then
  paired with the terminal's own default background too), but the overlay backgrounds are
  hardcoded dark colors; without an explicit foreground they'd render as dark-on-dark on a
  light-themed terminal, since syntax highlighting is off by default and the terminal's default
  foreground for a light theme is itself dark.
- Diff colors and cross-highlighting are painted onto the cached, already-highlighted lines as a
  second pass at render time (splitting/recoloring the relevant character spans), so the syntax
  foreground colors are preserved underneath.

## Screen state machine

`App` tracks one `AppScreen`: `Viewer` (default), `SelectFile`, `Diffing`. There is no separate
"overview" landing screen — `cargo run` starts directly on `Viewer`, showing the Before/After
panels empty (each with a "press 'o' to open a file" hint) rather than requiring a wizard before
anything is visible. Components are held as plain named fields on `App` (a `DiffViewer` and an
`Option<FileDialog>`) rather than a generic list dispatched by index; each screen activates a
specific component, and raw input events are only forwarded to that one active component, not
broadcast to every component regardless of visibility.

Pressing `o` opens a file dialog for whichever panel `DiffViewer::active_panel()` currently reports
(`Tab` switches this, in both dual- and single-panel display modes); the dialog's target panel is
recorded in `App::dialog_target`. Selecting a file loads it into that one panel only
(`DiffViewer::set_before_file`/`set_after_file`) and remembers the path in
`App::before_path`/`after_path`. The diff is (re-)computed automatically — moving to `Diffing` and
back to `Viewer` — whenever *both* paths are set after a pick, whether that's the first time both
sides are filled in or a later reselect of just one side. `Esc` cancels the active file dialog
instead of quitting the app while one is open (it quits the app from every other screen, alongside
`q`). A failed diff (e.g. an unsupported file extension) is reported via `App::last_error` and
rendered as a one-line red banner under the panels rather than failing silently.

## File dialog

A minimal, hand-rolled directory/file picker (no external file-picker dependency), consistent with
the rest of this codebase's widgets. Directory listings are read via `tokio::fs::read_dir` inside a
spawned task and delivered back through the normal action channel, so opening a directory never
blocks the render loop.

## Diff computation runs off the render thread

Parsing both files, running `Diff::from_code`, and building the `TextDiff` is CPU-bound and can be
slow for large files. `App::start_diff` runs this on a `tokio::task::spawn_blocking` thread and
reports the result back as `Action::DiffReady`/`Action::DiffFailed`. The diff pipeline is not
guaranteed panic-free for arbitrary input (e.g. it assumes a parsed AST further down the call
chain), so the blocking closure is wrapped in `std::panic::catch_unwind`; without this, a panic on
a `spawn_blocking` thread would silently vanish and leave the UI stuck on "Diffing…" forever.
Files whose extension isn't recognized (and therefore have no parsed AST) are rejected with a clear
`DiffFailed` message before any of this runs.

## Display mode threshold

`DiffViewer` switches from a side-by-side dual-panel layout to a single-panel (`Tab`-to-switch)
layout below a terminal width of 220 columns (`SINGLE_PANEL_THRESHOLD` in `diff_viewer.rs`) — wide
enough that each panel still gets a reasonably full-width view of real code in dual mode. In dual
mode, the active panel (the one `o` would target, and the one whose cursor drives navigation) is
shown with a thicker border and an inverted title, since the two panels are otherwise visually
identical (border color is fixed per side: red for Before, green for After) and there would
otherwise be no way to tell which one `Tab` last selected.

## Performance note

CPU usage should be judged from a `--release` build, not a debug build: idle CPU usage for an
unoptimized debug build of this TUI is roughly 30% (mostly the cost of unoptimized per-frame
buffer diffing at 60 fps), while the same idle scenario in a release build is roughly 3%.
