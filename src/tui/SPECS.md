# TUI specs

High-level architecture decisions and the rationale behind them. See the root README.md for the
overall Component architecture pattern this follows.

## Async event loop

All terminal input, ticks, and render timers are merged into a single `tokio::select!` loop in
`UI::event_loop` (`ui.rs`). Terminal input specifically comes from `crossterm::event::EventStream`
(an async, epoll-driven stream), not from manual polling on a dedicated OS thread. A prior version
of this code spawned a thread that called `crossterm::event::poll` with a zero timeout in a tight
loop, sleeping a few microseconds when idle; that burns a full CPU core continuously and was a
major source of the TUI feeling slow. There is now no dedicated input thread at all.

Tick rate and frame rate remain independently configurable (defaults: 4 Hz tick, 60 fps render),
each its own `tokio::time::interval` selected alongside the input stream.

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

- The cursor is an index into the focused panel's `Vec<RangeMatch>`. Up/Down move it to the
  previous/next entry, skipping zero-width entries (these are pure alignment markers with no real
  text on this side, used to keep both panels' range counts symmetric; they are not visually
  rendered).
- Moving the cursor recomputes `ranges[cursor].destination` and pushes it onto the *other* panel as
  its cross-highlight target. This is the only data needed to highlight "the matched node" on the
  other side: there is no separate node-matching search at the TUI layer.
- Background colors per `TextOperation`: `Insert` green, `Delete` red, `Move` yellow/olive,
  `Update` magenta, `Identical` left as plain syntax highlighting (the cursor entry instead gets a
  bold+underline marker so an unchanged selection is still visible). The cross-highlighted
  "matched node" on the non-focused panel uses a blue background, which takes priority over
  whatever diff-operation color that range would otherwise have, since it is a more specific,
  momentary signal than the static diff coloring.
- Diff colors and cross-highlighting are painted onto the cached, already-highlighted lines as a
  second pass at render time (splitting/recoloring the relevant character spans), so the syntax
  foreground colors are preserved underneath.

## Screen state machine

`App` tracks one `AppScreen`: `Overview`, `SelectBeforeFile`, `SelectAfterFile`, `Diffing`,
`ShowDiff`. Components are held as plain named fields on `App` (an `Overview`, a `DiffViewer`, and
an `Option<FileDialog>`) rather than a generic list dispatched by index; each screen activates a
specific component, and raw input events are only forwarded to that one active component, not
broadcast to every component regardless of visibility (the previous implementation forwarded every
key event to all components unconditionally, even when they weren't shown).

Pressing `d` from `Overview` or `ShowDiff` opens a file dialog for the "before" file; selecting a
file opens a second dialog for the "after" file; selecting that kicks off the diff computation and
moves to `Diffing`. `Esc` cancels the active file dialog instead of quitting the app while one is
open (it quits the app from every other screen, alongside `q`).

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

`DiffViewer` switches from a side-by-side dual-panel layout to a single-panel (Tab-to-switch)
layout below a terminal width threshold. This was previously set to 200 columns, which is wider
than most real terminals, so the dual-panel/cross-highlight view rarely actually appeared in
practice. It is now 120 columns.

## Performance note

CPU usage should be judged from a `--release` build, not a debug build: idle CPU usage for an
unoptimized debug build of this TUI is roughly 30% (mostly the cost of unoptimized per-frame
buffer diffing at 60 fps), while the same idle scenario in a release build is roughly 3%.
