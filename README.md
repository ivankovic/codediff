# CodeDiff

[![CI](https://github.com/ivankovic/codediff/actions/workflows/ci.yml/badge.svg)](https://github.com/ivankovic/codediff/actions/workflows/ci.yml)
[![Latest release](https://img.shields.io/github/v/release/ivankovic/codediff)](https://github.com/ivankovic/codediff/releases/latest)
[![docs.rs](https://docs.rs/codediff/badge.svg)](https://docs.rs/codediff)

Fast, robust, syntax aware code diffing.

# Installation

```
cargo install codediff
```

This builds from source, so you'll need a C compiler on `PATH` alongside a Rust toolchain
(edition 2024, so rustc 1.85+) - the tree-sitter grammars are all compiled from C during the
build. The first `cargo install` will take a couple of minutes because of this plus the
`lto = "fat"` release profile.

Prefer not to build from source? Pre-built binaries for Linux, macOS (Intel and Apple Silicon)
and Windows are attached to every [GitHub release](https://github.com/ivankovic/codediff/releases/latest).

The git-history analysis tools in `src/bin/` (not installed by `cargo install`, only relevant if
you're building from a checkout) sit behind an off-by-default `stats` feature, since they pull in
git2 and its own OpenSSL/libssh2 build dependencies that the diffing tool itself doesn't need. Build
them with `cargo build --features stats`.

# Using CodeDiff

## The interactive TUI

`codediff` with no arguments opens a terminal UI with two panels, "Before" and "After". Terminals
220 columns or wider show both panels side by side; narrower terminals show one panel at a time.
Passing two file paths, `codediff BEFORE AFTER`, opens straight into their diff instead of an empty
viewer. (Building from a checkout instead of a `cargo install`? Use `cargo run --` in place of
`codediff` in every example below.)

* `Tab` — switch the active panel.
* `o` — open a file selector for the active panel. Once both panels have a file, the diff between
  them is computed and drawn automatically (green = inserted, red = deleted, dark green = updated,
  yellow = moved).
* `c` — open the color theme picker. Built-in themes: Dark (default), Solarized Dark, Solarized
  Light, Dracula, Nord, Gruvbox Dark, Monokai, One Dark. The choice is remembered across runs.
* `?` — show every keybinding and the diff-color legend. `j`/`k` scrolls it, `?` or `Esc` closes it.
* Arrow keys or `h`/`j`/`k`/`l` — move the cursor, one line or column at a time, same as a text
  editor. The range under the cursor, and the matching range on the other panel, are highlighted
  in blue.
* `n`/`p` — jump straight to the next/previous change, skipping over unchanged lines entirely.
  Wraps around at the start/end of the file.
* `Page Up`/`Page Down`/`Home`/`End` — scroll.
* `q` or `Esc` — quit (`Esc` cancels an open dialog instead, while one is open).

If a diff turns out to involve two essentially unrelated files, full structural analysis can take
several seconds; codediff detects this and asks whether to wait for the precise (but slow) result
or accept a faster, approximate one instead.

## Headless / batch mode

`codediff --headless BEFORE AFTER` (or its synonym, `--batch`) prints the diff as plain, optionally
colored text instead of opening the TUI - for scripts, CI, or anywhere stdout isn't a real
terminal. This also kicks in automatically whenever stdout isn't a terminal (e.g. piped into
`less`, redirected to a file), so `codediff BEFORE AFTER | less` just works without the flag.

Long runs of unchanged lines are collapsed (3 lines of context on either side of a change, same
convention as `diff -u`), and each hunk is prefixed with the nearest enclosing function/class/
struct's own line when it isn't otherwise visible - so a change deep inside a large file still
tells you where it is, without printing the whole file around it.

By default, headless mode uses the same fast/approximate fallback the TUI can offer for unrelated-
looking files, without asking - pass `--exact` to always force the full, precise analysis instead.
Set `NO_COLOR=1` (<https://no-color.org>) to disable ANSI colors, e.g. when redirecting to a file.

## Git integration

`codediff` doubles as a `git difftool` backend:

```
git config difftool.codediff.cmd 'codediff "$LOCAL" "$REMOTE"'
git difftool --tool=codediff
```

Add `git config diff.tool codediff` to make plain `git difftool` (no `--tool` needed) use it by
default, and `git config difftool.prompt false` if you don't want git to ask "view diff ...
[Y/n]?" before every file.

It also works directly with `git diff`/`git log -p` via `GIT_EXTERNAL_DIFF` (no `difftool` config
needed, but always non-interactive - see "Headless / batch mode" above):

```
GIT_EXTERNAL_DIFF=codediff git diff
```

# Guiding principles

## Robust

CodeDiff must be able to process 100% of all commits in the full test dataset.

The full test dataset contains the git commit history, as available on the main branch, for about
7400 open source git repositories. The list of repositories was extracted from the Gentoo Linux
distribution and is available in `research/list_of_repositories.csv`.

A smaller list of 100 repositories, the "small" dataset, is available in the same directory for
faster iterations when debugging.

## Fast

CodeDiff must produce a diff in under 400ms for 99.99% of all commits in the full test dataset.

In code, I accept less readable, more complex code if it is faster.

Benchmarks are used to make sure performance doesn't regress.

# License

Copyright (C) 2026 Marko Ivankovic

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU Affero General Public License as published
by the Free Software Foundation.

See the LICENSE file for the full text of the License.

## Can't use AGPL software?

Contact me for options.

# AI policy

This project is developed with substantial AI assistance (currently Claude Code) - most commits
disclose this via a `Co-Authored-By` trailer and a link to the session that produced them. That's
not hidden, and it's not treated as a lesser way to write software here.

AI-assisted contributions are welcome. Use whatever tools help you do good work; disclose it the
same way, and you're still responsible for understanding and standing behind whatever you submit.

# For Developers, human or otherwise

See [CONTRIBUTING.md](CONTRIBUTING.md) for the technology overview, code quality/testing
expectations, project structure, and what CI checks on every push and PR. `AGENTS.md` has
additional AI-agent-specific conventions.
