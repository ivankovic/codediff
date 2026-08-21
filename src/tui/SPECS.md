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

## Git integration

The plain `codediff BEFORE AFTER` CLI (`src/main.rs`) doubles as a `git difftool` backend:
`git config difftool.codediff.cmd 'codediff "$LOCAL" "$REMOTE"'` needs no translation layer,
because git already substitutes `$LOCAL`/`$REMOTE` with real file paths before invoking the
command - temp copies of blobs for a commit-vs-commit diff, or the real working-tree file
directly when one side is the worktree - and (verified empirically against git 2.43, see
`Args::paths`' doc comment in `main.rs`) those temp copies keep the original basename/extension,
so `code::language::language_for_path` detects the language correctly with no extra plumbing.

`main.rs`'s `Args::paths` is a variable-length positional `Vec<PathBuf>` rather than two fixed
`Option<PathBuf>` fields specifically so a second calling convention can be recognized by argument
count alone: git's `GIT_EXTERNAL_DIFF` (invoked directly by `git diff`/`git log -p --ext-diff`/etc.,
not just `difftool`) calls the external diff command with 7 positional arguments, `path old-file
old-hex old-mode new-file new-hex new-mode` - `resolve_before_after` picks `old-file`/`new-file`
(indices 1 and 4) out of that shape and ignores the rest, tested in `main.rs`'s
`tests::resolve_before_after_*`.

That count is 9, not 7, when git detected the change as a rename or a copy: it appends `other` (the
destination path) and a rename/copy score. `old-file`/`new-file` stay at indices 1 and 4 - the
shape is a suffix of the 7-argument one - so both counts resolve identically and the two extra
arguments are ignored like the hex/mode fields. This is not an exotic case: `diff.renames` defaults
to on for `git diff`, so every commit containing a rename hits it, and rejecting the count exited
non-zero, which git reads as "external diff died" and abandons the whole run over. Argument count
is also how `invoked_as_git_external_diff` recognizes this convention for the exit-code invariant
(`exit_code_for`) and the binary notice's wording, so all of them accept 7 or 9 through that one
predicate rather than each testing a literal.

Both conventions can hand codediff `/dev/null` for one side, representing an added or deleted
file (git's own behavior, not something either integration path chooses). `/dev/null` reads back
as valid, empty content, but has no extension for `language_for_path` to key off, so `compute_diff`
(`tui/app.rs`) special-cases an empty, language-less side: it re-parses that side as empty content
in the *other* side's already-detected language instead of bailing with "unsupported or
unrecognized file type", which turns into a normal whole-file insert/delete in the resulting diff.
(`Code::from_string` also had to stop unconditionally calling `compute_ast_metadata` for a
language-less, unparsed `Code` - it's an expected state here, not a bug, and the unconditional call
logged a spurious "Failed to compute AST metadata" line to stderr on every such diff.)

### Binary files

`Code::from_file` reads with `read_to_string`, so a binary side (a PDF, an image) fails the UTF-8
decode. Left to propagate, that error exits 2 - and git reads *any* non-zero exit from an external
diff as "external diff died", abandoning the whole run: one PDF in a commit used to take every
file after it in the sort order down with it, `git diff` printing a `fatal:` and stopping. So
`main.rs` checks `code::is_binary_file` on both sides *before* the json/headless/TUI split (a
binary side has the same non-answer in all three, and the interactive viewer cannot show one
either) and prints a one-line `Binary file <path> differs` stand-in, exiting through the same
`exit_code_for` as every other non-interactive path. `--mode json` gets the same object shape it
always does with a `binary` flag set instead, since a prose sentence on stdout would break every
consumer of that mode.

`is_binary_file` is deliberately not a heuristic of its own - not git's "NUL byte in the first 8KB"
check - but `from_file`'s own failure condition run against the same bytes. Two independent
heuristics agree only by coincidence: a Latin-1-encoded source file has no NUL byte, so git's check
calls it text while `read_to_string` still fails on it, which would land a caller right back on the
error path this exists to avoid. `code.rs`'s tests assert the agreement in both directions rather
than the classification alone.

Only *one* side has to be binary. A binary file being added or deleted arrives as `/dev/null`
opposite the blob, and `/dev/null` reads back as empty, perfectly valid UTF-8.

Note the scope: this covers files codediff cannot *decode*. A file it cannot *open* (missing, no
permission) is still a genuine error, still exits 2, and under `GIT_EXTERNAL_DIFF` will still stop
the run.

### Headless/text mode: `tui::headless`

git's default pager means a full-screen TUI can't just be used as `GIT_EXTERNAL_DIFF` unmodified:
git pipes its own stdout through the pager, so codediff's stdout is a pipe, not a real terminal,
and a full-screen TUI cannot draw onto a pipe. `git difftool` doesn't have this problem (it never
involves a pager in the first place, which is the main reason it was implemented first) - but
`GIT_EXTERNAL_DIFF` does, and so does any other non-interactive invocation (`codediff a b > out`,
CI, ...). Rather than requiring the caller to know to pass `--no-pager`/`GIT_PAGER=cat`, `main.rs`
detects this itself: `should_run_headless` checks `std::io::stdout().is_terminal()` (in addition to
the pre-existing explicit `--headless`/`--mode headless` flags) and falls back to `tui::headless`
whenever stdout isn't a real terminal, regardless of *why* it isn't - the tty check is the actual
root cause, not any git-specific signal, so it covers every non-interactive case uniformly.

`tui::headless::run` calls the exact same `app::compute_diff` the TUI uses (same parsing, same
`ASTDiff`, same `TextDiff`) and renders the result as text instead of drawing it. The renderer
(`render_text_diff`/`render_side`/`row_operations`) is deliberately row-granular, not the TUI's
column-precise overlay: `diff::text`'s ranges are whitespace-insensitive and can leave small gaps
(see `python_leetcode_1_added_if_block_all_ranges` in `diff/text.rs`), so reconstructing exact
sub-line spans for a plain-text renderer would be fragile. Each output line gets whichever
non-`Identical` operation touches its row (`row_operations`'s precedence rule exists specifically
so that an `Identical` range sharing a row with a real change - e.g. the punctuation around a
renamed single token - can't silently overwrite that change back to plain; caught by an end-to-end
smoke test against the built binary, not code review). Colors match the TUI's own convention
(insert green, delete red, move yellow, update magenta) via ANSI escapes, on by default since the
main use case (piping into git's pager) is generally able to render them - the `NO_COLOR`
convention (<https://no-color.org>) is the escape hatch for callers that don't want that, e.g.
redirecting to a file. Not implemented (rendering a true interleaved unified-diff hunk format
merging both sides into one stream) since it would need to invent new merge semantics on top of
`diff::text`'s bespoke range model beyond what either integration path actually requires today.

Before the two rendered sides, `render_text_diff` prints the same one-line diff-shape
classification the interactive TUI shows in its status bar (`diff::text::DiffSummary`, via
`app::status_bar_paragraph`'s underlying `summarize_diff_with_comment_check`) - "No changes",
"Comment changes only", "Whitespace changes only", etc. - bolded rather than colored (so it still
stands out with `NO_COLOR`), and only when the diff actually classifies as one of those special
cases; an ordinary mixed edit gets no extra line at all. `tui::json_output`'s JSON mode carries the
identical classification as an optional `summary` field (a snake_case tag, e.g. `"comment_only"`,
omitted entirely rather than `null` for the ordinary case) instead of a printed line, for the same
reason it has a `fallback_used` field instead of `headless::run`'s stderr note: JSON mode is for a
script to parse, not a person to read.

## Performance note

CPU usage should be judged from a `--release` build, not a debug build: idle CPU usage for an
unoptimized debug build of this TUI is roughly 30% (mostly the cost of unoptimized per-frame
buffer diffing at 60 fps), while the same idle scenario in a release build is roughly 3%.
