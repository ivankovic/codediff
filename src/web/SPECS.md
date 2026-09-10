# Web front end specs

Design decisions behind `codediff-web`, and the log of why each was taken. See the root
README.md for the component architecture the rest of the project follows, and `src/tui/SPECS.md`
for the terminal front end this one mirrors.

## What it is

`codediff-web` is the interactive viewer served to a browser instead of drawn on a terminal. It is
a separate binary (`src/web_main.rs`), behind the `web` cargo feature, off by default, so
`cargo install codediff` keeps producing exactly the one binary it always has. It never runs the
`codediff` binary: the diff comes from the same library calls the TUI makes
(`tui::app::compute_diff_with_options`), through the same config (`tui::theme`) and the same
syntect setup (`tui::widgets::code_viewer`), so the two front ends cannot disagree about a diff.

The target is feature parity with the TUI. Every key listed in the TUI's `?` help does the same
thing in the page - the help text itself is served from the one constant the TUI draws
(`HELP_TEXT`), and the footer hint line is pinned to the TUI's by a Rust test.

## Where the state lives

The split follows the TUI's own: what `App` and `DiffViewer` keep *between* keystrokes that is not
view state lives on the server (`web::session::Session`); everything the eye sees moving lives in
the page (`assets/web/model.js`, a port of `tui::widgets::code_viewer` and
`tui::components::diff_viewer`).

Server-side: the open pair, the persisted settings, the last computed diff kept *unfiltered* (as
`DiffViewer::full_ranges` is) so a render-options change that only re-filters needs no new diff,
and the generation counter that makes cancellation work. Page-side: cursor, scroll, focus, the
merged change walk, search, every dialog, and the overlay painting order.

The consequence is that the server never learns where the cursor is - with one exception, `e`,
which needs a path and a line number and gets them in the request.

## No server crate

The HTTP layer (`web::http`) is written here: a request line, a header block, a `Content-Length`
body, one response per connection. Two hundred lines with tests. A server crate would carry its own
dependency graph - `axum` about thirty crates, `tiny_http` a handful - and every crate in
`Cargo.lock` is a line in `packaging/gentoo`'s generated `CRATES=` block and an entry in its
`LICENSE` enumeration, whether or not the feature that needs it is enabled. `tokio` is already there
for the TUI. Result: `Cargo.lock` is byte-for-byte unchanged by this feature, and so is the ebuild.

Deliberately unsupported, because nothing sends it: chunked request bodies (a browser's `fetch`
with a string body always sends `Content-Length`), keep-alive (every response says
`Connection: close`, which every browser honours), pipelining, HTTP/2. Requests outside the subset
get a 4xx rather than a misreading.

## Columns are UTF-16 on the wire

Everything else in this crate works in bytes (`diff::text_range`), and `tui::json_output`
documents that its columns are bytes because Neovim consumes them directly. A browser indexes a
string by UTF-16 code units, and the page does every cursor step, range lookup and paint against
the strings it holds. So `web::payload` converts every column once, against the real line text,
before it leaves the server; the page never converts. The two JSON outputs serve different
consumers and are deliberately different.

Syntax highlighting rides along the same way: syntect's per-line regions become colour spans in
UTF-16 columns, adjacent same-colour runs merged. Only the foreground is carried, which is all the
TUI applies. The theme's own background and foreground come too, and become the page's base
colours - a light syntect theme on a dark page would be unreadable, and the TUI has the terminal's
colours for this.

## Two checks on every request

A server on localhost is reachable by every page the user has open in the same browser. Two
checks keep the viewer from being driven by a page that is not it:

1. `Host` must name this server (a loopback name or the bound host, at the bound port). Otherwise
   a hostile page could point a name it controls at 127.0.0.1 (DNS rebinding), load `/` as if it
   were same-origin, and read the token out of it.
2. Every `/api/` call must carry the per-run token the page received in `/`, in a custom header.
   A custom header makes the browser ask permission first (a CORS preflight), which this server
   never grants - so a foreign page can neither read a diff nor make the session diff, quit, or
   launch an editor.

The API is `POST` + JSON throughout, reads included: one shape, one parser, and no endpoint a
plain `<img src>` or link could trigger. The token comes from the standard library's randomly
keyed hasher rather than a `rand` dependency (`rand` is `stats`-only here on purpose).

## Cancellation

Esc during "Diffing…" aborts the page's request and tells the server to retire the generation.
The computation itself cannot be killed - the same limitation `App::start_diff` documents - so the
session accepts only the result whose generation is still current and drops any other when it
arrives. A newer request supersedes an older one the same way.

## Dialogs and screens

Each of the TUI's screens has a counterpart with the same keys: the file dialog (full-page, like
the TUI's), the theme dialog with its two dropdowns and nine editable colours, the render-options
panel with its `1`/`2` presets and Esc-restores-what-it-opened-with rule, help, search with the
live match count, the jump-to-line prompt, the recent-pairs overlay on the empty start screen, the
summary toast and status bar, the error banner, and the "Diffing…" screen with its elapsed time.

Settings are persisted through the same `tui::theme::save_*` calls the TUI's dialogs make, so the
two front ends read and write one config file, project-layered exactly as `tui::theme` resolves
it.

`e` runs `$VISUAL`/`$EDITOR` inheriting the server's stdio, so a terminal editor opens in the
terminal `codediff-web` was started from and a GUI editor opens wherever it opens; the page
re-diffs when it exits, keeping the cursor.

`q` stops the server, which is what quitting the TUI does. There is no automatic stop when the
tab closes: a reload would kill the session, and a `git difftool` driving this binary wants it to
outlive one page load.

## Not carried over

* Ctrl-Z. A browser tab has no job control to suspend into.
* Mouse drag selection is the browser's own, as it is the terminal's in the TUI.

## TUI behaviour noticed while porting, kept as is

These are the TUI's behaviours, ported faithfully so the two agree; each is arguably a defect and
is listed in TODO.md rather than fixed here, since fixing them is a TUI decision.

* The merged `change N/M` counter orders stops by (panel, position), not by the order `n` walks
  them in, so it does not climb monotonically across a panel switch.
* The footer's `[... off]` badge names "Whole-pair updates" whenever anything else is off, because
  that option is off in the FULL preset itself.
* In the TUI, `q` quits from every screen, including the search modal and the file dialog's filter
  - typing a `q` there ends the session. The page deliberately does not do this: `q` quits from the
  viewer, the help modal and the diffing screen only.

## Testing

`web::http`, `web::payload`, `web::session` and `web::server` are unit-tested in Rust, the server
against a real listener on a random port. `assets/web/model.js` is tested under plain Node
(`make test-web-js`), each block mirroring a test in the TUI's widgets, so the page's behaviour is
pinned to the terminal's. `app.js` - measuring, drawing, fetching - has no automated coverage;
like `assets/mapping_site/`'s DOM code, nothing here can run it outside a browser.
