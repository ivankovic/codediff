# CodeDiff

[![CI](https://github.com/ivankovic/codediff/actions/workflows/ci.yml/badge.svg)](https://github.com/ivankovic/codediff/actions/workflows/ci.yml)
[![Latest release](https://img.shields.io/github/v/release/ivankovic/codediff)](https://github.com/ivankovic/codediff/releases/latest)
[![docs.rs](https://docs.rs/codediff/badge.svg)](https://docs.rs/codediff)

Fast, robust, syntax-aware code diffing.

# Installation

```
cargo install codediff
```

This command builds CodeDiff from source. You need a C compiler on `PATH` and a Rust toolchain,
edition 2024 or later (rustc 1.85 or later). The build compiles every tree-sitter grammar from C.
The first `cargo install` takes a few minutes, because of this and the `lto = "fat"` release
profile.

Pre-built binaries for Linux, macOS (Intel and Apple Silicon), and Windows are attached to every
[GitHub release](https://github.com/ivankovic/codediff/releases/latest).

The git-history analysis tools in `src/bin/` sit behind an off-by-default `stats` feature.
`cargo install` does not install these tools. They matter only if you build from a checkout. The
`stats` feature exists because these tools need git2 and its own OpenSSL/libssh2 build
dependencies. The diffing tool itself does not need these dependencies. Build these tools with
`cargo build --features stats`.

# Using CodeDiff

## The interactive TUI

`codediff` with no arguments opens a terminal UI with two panels, "Before" and "After". Terminals
220 columns or wider show both panels side by side. Narrower terminals show one panel at a time.
`codediff BEFORE AFTER`, with two file paths, opens directly into their diff, not an empty viewer.
If you build from a checkout instead of `cargo install`, use `cargo run --` in place of `codediff`
in every example below.

* `Tab` — switch the active panel.
* `o` — open a file selector for the active panel. Once both panels have a file, codediff computes
  and draws the diff between them automatically. Green means inserted. Red means deleted. Dark
  green means updated. Yellow means moved.
* `c` — open the color theme picker. Built-in themes: Dark (default), Solarized Dark, Solarized
  Light, Dracula, Nord, Gruvbox Dark, Monokai, One Dark. codediff remembers your choice across runs.
* `?` — show every keybinding and the diff-color legend. `j`/`k` scrolls it. `?` or `Esc` closes it.
* Arrow keys or `h`/`j`/`k`/`l` — move the cursor, one line or column at a time, same as a text
  editor. codediff highlights the range under the cursor, and the matching range on the other
  panel, in blue.
* `n`/`p` — jump straight to the next or previous change. This skips unchanged lines entirely. It
  wraps around at the start or end of the file.
* `/` — search the focused panel for text. `Enter` jumps to the nearest match and highlights every
  match in blue. `Esc` cancels; an empty query clears the current search's highlights. `>`/`<` step
  to the next/previous match once a search is active.
* `Page Up`/`Page Down`/`Home`/`End` — scroll.
* `q` or `Esc` — quit. If a dialog is open, `Esc` cancels the dialog instead.

If a diff involves two unrelated files, full structural analysis can take several seconds.
codediff detects this case. It asks whether to wait for the precise, slow result or accept a
faster, approximate result instead.

## Headless / batch mode

`codediff --headless BEFORE AFTER`, or its synonym `--batch`, prints the diff as plain text, with
optional color, instead of opening the TUI. Use this for scripts, CI, or any case where stdout is
not a real terminal. Headless mode also starts automatically whenever stdout is not a terminal, for
example when piped into `less` or redirected to a file. Because of this, `codediff BEFORE AFTER |
less` works without the flag.

codediff collapses long runs of unchanged lines. It keeps 3 lines of context on each side of a
change, the same convention as `diff -u`. codediff also prefixes each hunk with the nearest
enclosing function, class, or struct line, when that line is not otherwise visible. This shows the
location of a change deep inside a large file. The reader does not need to see the whole file
around it.

By default, headless mode uses the same fast, approximate fallback that the TUI offers for
unrelated-looking files, without asking. Pass `--exact` to force the full, precise analysis
instead. Pass `NO_COLOR=1` (see <https://no-color.org>) to disable ANSI colors, for example when
you redirect output to a file.

## Git integration

`codediff` doubles as a `git difftool` backend:

```
git config difftool.codediff.cmd 'codediff "$LOCAL" "$REMOTE"'
git difftool --tool=codediff
```

Run `git config diff.tool codediff` to make plain `git difftool` use codediff by default, without
needing `--tool`. If you do not want git to ask "view diff ... [Y/n]?" before every file, run `git
config difftool.prompt false`.

codediff also works directly with `git diff` and `git log -p`, through `GIT_EXTERNAL_DIFF`. This
path needs no `difftool` config, but it is always non-interactive (see "Headless / batch mode"
above):

```
GIT_EXTERNAL_DIFF=codediff git diff
```

# Guiding principles

## Robust

CodeDiff must process 100% of all commits in the full test dataset.

The full test dataset holds the git commit history of about 7,400 open-source git repositories,
as available on the main branch. This list of repositories comes from the Gentoo Linux
distribution. Find it in `research/list_of_repositories.csv`.

A smaller list of 100 repositories, the "small" dataset, is available in the same directory. Use
it for faster iteration when you debug.

## Fast

CodeDiff must produce a diff in under 400ms for 99.99% of all commits in the full test dataset.

In code, I accept less readable, more complex code, if that code is faster.

Benchmarks make sure that performance does not regress.

# License

Copyright (C) 2026 Marko Ivankovic

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU Affero General Public License as published
by the Free Software Foundation.

See the LICENSE file for the full text of the License.

## Cannot use AGPL software?

Contact me for options.

# AI policy

This project uses substantial AI assistance, currently Claude Code. Most commits disclose this
with a `Co-Authored-By` trailer and a link to the session that produced them. This project does not
hide that fact. This project does not treat AI assistance as a lesser way to write software.

AI-assisted contributions are welcome. Use whatever tools help you do good work. Disclose your use
of these tools the same way. You are still responsible for understanding and standing behind
whatever you submit.

# For Developers, human or otherwise

See [CONTRIBUTING.md](CONTRIBUTING.md) for the technology overview, code-quality and testing
expectations, project structure, and what CI checks on every push and PR. `AGENTS.md` has
additional AI-agent-specific conventions.
