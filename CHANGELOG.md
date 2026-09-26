# Changelog

All notable changes to CodeDiff. The format follows [Keep a Changelog](https://keepachangelog.com/),
and the version numbers follow [Semantic Versioning](https://semver.org/) as far as a 0.x
release does: a minor bump may change the JSON output or the library API, a patch bump does not.

## [0.1.1] - unreleased

### Changed

- Tabs display at tab stops, in the TUI and in headless output, instead of as one space.
- The TUI uses the nearest 256 colors when the terminal does not advertise 24-bit color (macOS
  Terminal.app).
- `--minimal`, `--full`, `--whole-updates` and `--paint-reindent-moves` apply in the TUI too, for
  that run only.
- `--review` rejects a BEFORE/AFTER pair and `--mode`, and `--headless` rejects `--mode`, instead
  of ignoring one flag.
- `codediff git configure` writes the resolved path of the running binary to `diff.external`.
- `change N/M` counts changes in the order `n` walks them.
- The render-options badge names what differs from `--full`.
- Panel titles name languages as people write them (C++, Lua, Markdown). The JSON `language`
  values are unchanged.
- Library: the `diff::solve_*` pass modules are crate-private, `ASTDiff::is_valid` has no `after`
  parameter, and `NodeCache` takes a lifetime (see Fixed). **Breaking** for library users, against
  the policy above; the library API was never meant to be relied on (see the crate docs).

### Fixed

- The TUI cursor lands on the right column on lines with non-ASCII text.
- Ctrl-C quits from every screen, and Ctrl-E scrolls instead of opening `$EDITOR`.
- The theme dialog opens on the syntax theme in use, and accepting a preset keeps the saved Custom
  palette.
- Home and End move the cursor, not only the view.
- `$VISUAL`/`$EDITOR` may carry arguments (`code -w`, `emacsclient -t`).
- Omitting `--whole-updates` no longer switches a saved "Whole-pair updates" setting off.
- The TUI exits when its input stream closes.
- Library: `NodeCache` borrows the `Code` it was built from, so a cache that outlives its `Code`
  no longer compiles; before, safe code could read freed memory through it.

### Removed

- The browser front end (`codediff-web`) and the `web` Cargo feature. **Breaking** for anyone who
  installed with `cargo install codediff --features web`; for reviewing changes in a browser, use
  [codereview](https://github.com/ivankovic/codereview). The GitHub Pages showcase is unaffected.

## [0.1.0] - 2026-09-25

The first release ready for users!

Ready for:

-  Daily usage as a `git difftool` tool!
-  Daily usage in an IDE!
-  Integration into batch pipelines and LLM agents!

Things that you can do, but expect changes:

-  Integration as a library into other products. The API is not fixed and might change.

### Metrics:

-  Speed: p50 7.6ms, p90 78.7ms, p99 347ms, slowest 1,355ms. 100ms is the 92.7th percentile
-  Robust: 99.95%
-  Accurate: 70.6% perfect, 88.6% <= 1% off

## [0.0.14] and earlier

Pre-releases; see the [GitHub releases](https://github.com/ivankovic/codediff/releases).
