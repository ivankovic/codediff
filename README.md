# CodeDiff

[![CI](https://github.com/ivankovic/codediff/actions/workflows/ci.yml/badge.svg)](https://github.com/ivankovic/codediff/actions/workflows/ci.yml)
[![Latest release](https://img.shields.io/github/v/release/ivankovic/codediff)](https://github.com/ivankovic/codediff/releases/latest)
[![docs.rs](https://docs.rs/codediff/badge.svg)](https://docs.rs/codediff)
[![coverage](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/ivankovic/codediff/main/research/data/coverage/badge.json)](CONTRIBUTING.md#coverage)
[![License: AGPL v3+](https://img.shields.io/badge/license-AGPL--3.0--or--later-blue)](LICENSE)

Fast, robust, accurate, syntax-aware code diffing.

![An animation of one Python refactoring painted two ways. A vertical bar sweeps left to right and
back across a two-pane diff. On one side of the bar, GNU diff marks whole lines as deleted and
inserted; on the other, CodeDiff paints only the parts that changed - `sum(numbers)` and
`len(numbers)` rather than the whole assignment, and `numbers` shown as moved rather than
rewritten.](/assets/diff-vs-codediff.gif)

**[Try it in the browser](https://ivankovic.github.io/codediff/showcase/)**: twenty real changes,
compared side by side in Unix `diff` and in CodeDiff.

Light theme is available:

![A screenshot of CodeDiff's two-panel terminal UI in a light theme, showing the same Python
refactoring, with the changed right-hand sides highlighted rather than whole
lines](/assets/readme-screenshot.png)

# Installation

## From source

```
cargo install codediff
```

This command builds CodeDiff from source. You need a C compiler on `PATH` and a Rust toolchain,
edition 2024 or later (rustc 1.88 or later). The build compiles every tree-sitter grammar from C.
The first `cargo install` takes a few minutes, because of this and the `lto = "fat"` release
profile.

## Prebuilt binaries

Pre-built binaries for Linux, macOS (Intel and Apple Silicon), and Windows are attached to every
[GitHub release](https://github.com/ivankovic/codediff/releases/latest).

## Homebrew

macOS and Linux:

```
brew install ivankovic/codediff/codediff
```

## Debian and Ubuntu

`.deb` packages are available in a signed apt repository, so `apt upgrade` picks up new
versions like any other package. amd64 and arm64:

```sh
sudo install -d -m 0755 /etc/apt/keyrings
curl -fsSL https://ivankovic.github.io/codediff/apt/codediff-archive-keyring.gpg \
  | sudo tee /etc/apt/keyrings/codediff-archive-keyring.gpg > /dev/null
echo "deb [signed-by=/etc/apt/keyrings/codediff-archive-keyring.gpg] \
https://ivankovic.github.io/codediff/apt stable main" \
  | sudo tee /etc/apt/sources.list.d/codediff.list > /dev/null
sudo apt update && sudo apt install codediff
```

## Nix and NixOS

On NixOS, or anywhere with Nix installed, no installation step is needed at all:

```
nix run github:ivankovic/codediff
```

## Arch and Gentoo

Recipes for Arch (AUR) and Gentoo live in [`packaging/`](packaging/). Neither is submitted to its
distribution's repository yet.

## Editor integration

**Please note: VS Code doesn't yet support replacing the default diff. The API feature request for
this functionality is currently implemented but not yet released. As soon as it is released,
CodeDiff will support replacing the default VS Code diff**

* **VS Code** - [codediff-vscode](https://github.com/ivankovic/codediff-vscode), v0.0.1. Search for
  **CodeDiff** in the Extensions view, or `code --install-extension ivankovic.codediff`. Also on
  [Open VSX](https://open-vsx.org/extension/ivankovic/codediff) for VSCodium, Cursor and Windsurf.
* **Neovim** - [codediff.nvim](https://github.com/ivankovic/codediff.nvim).

# Using CodeDiff

## The interactive TUI

```
codediff                  # open an empty viewer; press o to pick each file
codediff BEFORE AFTER     # open directly into the diff of two files
```

Press `?` in the viewer for the full list of keybindings.

The TUI uses 24-bit color when the terminal advertises it with `COLORTERM=truecolor`, and the
nearest 256 colors otherwise (macOS Terminal.app, or most terminals over ssh, which does not
forward `COLORTERM`). If your terminal supports 24-bit color but does not set it, run
`export COLORTERM=truecolor`.

## The web UI

`codediff-web` is the same viewer served to a browser tab. It is behind the off-by-default `web`
feature:

```
cargo install codediff --features web
codediff-web BEFORE AFTER
```

It starts a local server, prints the URL, and opens your browser (`--no-open` to skip that,
`--port` and `--host` to pick where it listens - the default is a random port on 127.0.0.1). With
no arguments it starts empty, like the TUI, and it accepts git's `GIT_EXTERNAL_DIFF` argument list
too.

The web UI is available, but not recommended.

## Headless / batch mode

`codediff --headless BEFORE AFTER`, or its synonym `--batch`, prints the diff as plain text, with
optional color, instead of opening the TUI. Use this for scripts, CI, or any case where stdout is
not a real terminal. Headless mode also starts automatically whenever stdout is not a terminal, for
example when piped into `less` or redirected to a file. Because of this, `codediff BEFORE AFTER |
less` works without the flag.

Every printed line is prefixed with its line number, so the moved-chunk headers' "Moved to lines
40-60" cross-references can actually be followed. CodeDiff collapses long runs of unchanged
lines. It keeps 3 lines of context on each side of a change (override with `--context N`), the
same convention as `diff -u`. CodeDiff also prefixes each hunk with the nearest enclosing
function, class, or struct line, when that line is not otherwise visible. This shows the location
of a change deep inside a large file.

Colors are on by default (git's pager renders them); pass `--color never`, or set `NO_COLOR=1`, to
disable ANSI colors, for example when you redirect output to a file - `--color always` forces them
even under `NO_COLOR`.

CodeDiff exits `0` on success and `2` on error. For scripting, pass `--exit-code` to additionally
get `1` when the files differ, the `diff(1)` convention. That is opt-in rather than the default
for the same reason `git diff` exits `0` even when files differ: CodeDiff's usual non-interactive
callers are version control systems driving it as a display tool, and they read a non-zero exit as
"the tool failed" - `jj` warns on every file, and `git difftool` with `difftool.trustExitCode=true`
aborts the whole diff. (The 7-argument `GIT_EXTERNAL_DIFF` form stays at `0` even with
`--exit-code`, since git treats a non-zero exit there as fatal.)

## JSON output

`codediff --mode json BEFORE AFTER` prints the diff as one JSON object, for editors and tools that
place highlights on their own buffers. Each side carries its path, its detected language and its
hunks, and each hunk is an operation (`delete`, `insert`, `update`, `move`) with a range in that
side's own file:

```json
{
  "before": {
    "path": "old.rs",
    "language": "Rust",
    "hunks": [
      { "operation": "delete", "range": { "start_row": 12, "start_column": 4, "end_row": 12, "end_column": 20 } }
    ]
  },
  "after": { "path": "new.rs", "language": "Rust", "hunks": [ ... ] },
  "large_residual": false,
  "summary": "comment_only"
}
```

Rows and columns are 0-indexed, and columns are byte offsets within their row, as tree-sitter
reports them. `summary` is present only when the diff is one of the special shapes the TUI's
status bar names, such as `comment_only` or `whitespace_only`. A binary file on either side
answers with `"binary": true` and empty hunks. Unlike headless mode, JSON output is never chosen
automatically: only `--mode json` selects it, so a pipe never receives it by surprise. The
[VS Code extension](https://github.com/ivankovic/codediff-vscode) is built on this output; the
authoritative field list is `src/tui/json_output.rs`.

## Git integration

CodeDiff is a `git difftool` backend. Run the interactive setup wizard, which asks
whether to configure it globally or for the current repository only:

```
codediff git configure
```

Or configure it by hand:

```
git config difftool.codediff.cmd 'codediff "$LOCAL" "$REMOTE"'
git difftool --tool=codediff
```

Run `git config diff.tool codediff` to make plain `git difftool` use CodeDiff by default, without
needing `--tool`. If you do not want git to ask "view diff ... [Y/n]?" before every file, run `git
config difftool.prompt false`.

**`git difftool` opens the interactive TUI. `git diff` and `git log -p` never do.** `git diff`
pipes its output through git's pager, and a full-screen TUI cannot draw onto a pipe, so CodeDiff
always falls back to plain text there regardless of terminal or `GIT_EXTERNAL_DIFF` config (see
"Headless / batch mode" above). If you want the interactive viewer from git, use `git difftool`,
not `git diff`. If `git difftool` still doesn't open interactively over SSH, reconnect with
`ssh -t` — the session needs an allocated pseudo-terminal; tmux panes always have one.

CodeDiff also works directly with `git diff` and `git log -p`, through `GIT_EXTERNAL_DIFF`. This
path needs no `difftool` config:

```
GIT_EXTERNAL_DIFF=codediff git diff
```

Binary files - anything CodeDiff cannot read as text, a PDF or an image - get a one-line
`Binary file <path> differs` notice instead of a diff, the same stand-in git and `diff(1)` print
for them. They likewise never block the rest of a `git diff`: an external diff that exits non-zero
makes git abandon the *entire* run, so CodeDiff reports an unshowable file as a successful diff of
nothing rather than as a failure.

## Jujutsu (jj) integration

**Note:** jj support will be much improved in v0.2.*.

jj does not read git's `difftool`/`diff.external` settings, even in a colocated repo, so it needs
its own configuration. Run the setup wizard:

```
codediff jj configure
```

Or configure it by hand:

```
jj config set --user merge-tools.codediff.program codediff
jj config set --user merge-tools.codediff.diff-args '["$left","$right"]'
jj config set --user merge-tools.codediff.diff-invocation-mode file-by-file
```

That registers `jj diff --tool codediff`. To make it the default for plain `jj diff` as well:

```
jj config set --user ui.diff-formatter codediff
```

Use `--repo` in place of `--user` to configure the current repository only.

**`diff-invocation-mode = "file-by-file"` is required.** jj's default hands a diff tool two
*directory* trees; CodeDiff diffs two files, so without this setting every invocation fails. With
it, jj passes one changed file pair at a time, keeping each file's real path and extension, so
language detection works exactly as it does under git.

`jj diff` runs its formatter under a pager, so CodeDiff renders in its non-interactive text mode
there - the same output `git diff` gets. jj has no equivalent of `git difftool`'s interactive
per-file viewer (its terminal-attached hook, `ui.diff-editor`, is for `jj diffedit`/`jj split`,
which edit the right-hand side and read it back - not something a read-only viewer should claim to
do), so for the full-screen TUI on a jj repo, run `codediff BEFORE AFTER` directly.

# Supported languages

The language is detected from the file extension. A file with an unknown extension is diffed as
plain text, line by line, so nothing is refused.

<!-- languages:start -->
24 languages are parsed with a tree-sitter grammar and diffed structurally:

| Language | File extensions |
|---|---|
| C | `.c`, `.h` |
| C# | `.cs` |
| C++ | `.cc`, `.cpp`, `.cxx`, `.hpp`, `.hh`, `.hxx` |
| CSS | `.css`, `.scss` |
| Go | `.go` |
| HTML | `.html`, `.htm` |
| Java | `.java` |
| JavaScript | `.js`, `.mjs`, `.cjs`, `.jsx` |
| JSON | `.json` |
| Kotlin | `.kt` |
| Lua | `.lua` |
| PHP | `.php` |
| Python | `.py`, `.pyi`, `.pyw` |
| R | `.r` |
| Ruby | `.rb` |
| Rust | `.rs` |
| Scala | `.scala` |
| Shell (Bash) | `.bash`, `.sh` |
| Swift | `.swift` |
| TSX | `.tsx` |
| TypeScript | `.ts`, `.mts`, `.cts` |
| Vimscript | `.vim` |
| XML | `.xml`, `.xht`, `.xhtml` |
| YAML | `.yaml`, `.yml` |

Recognised by extension but diffed as plain text, since no grammar is compiled in: Bazel (`.bazel`), Dart (`.dart`), Emacs Lisp (`.el`), Markdown (`.md`, `.markdown`), Protocol Buffers (`.proto`), SQL (`.sql`).
<!-- languages:end -->

# Guiding principles

## Fast

CodeDiff's goal is:

* **A median diff in 100ms or less.**
* **A 99th-percentile diff in 1000ms or less.**

Both are met. Over the 2,001 fixtures in `src/test/data/diffs/`, measured on an Intel Xeon
E3-1275 v5 (4 cores, 8 threads) with 64 GB RAM: **p50 7.6ms, p90 78.7ms, p99 347ms**, slowest
1,355ms. **100ms is the 92.7th percentile** — 146 of 2,001 fixtures take longer than that, and 2
take longer than a second.

Benchmarks make sure that performance does not regress. `make benchmark-quality` prints the
distribution above; `make check-quality` compares it against the committed baseline on every push.

## Robust

CodeDiff's goal is to process 100% of all commits.

The full test dataset holds the git commit history of about 7,400 open-source git repositories,
as available on the main branch. This list of repositories comes from the Gentoo Linux
distribution. Find it in `list_of_repositories.csv`.

Measured over every modified code file in the most recent 50 commits of each of those
repositories - 442,530 readable before/after pairs in 25 languages, diffed with no size cap under
a 120-second budget and a 6 GB memory cap per process - **442,322 (99.95%) completed
successfully.** 96 pairs ran past the budget and 112 past the memory cap; the latter are twenty
generated or embedded files - tree-sitter parser tables, codegen, minified bundles, a PNG as a C
array - plus one commit of a 40,000-line single-header C++ library. Given 24 GB, eleven of those
files complete. The run, its harness and every pair that did not complete are documented in
`research/data/performance/PROVENANCE.md`.

## Accurate

CodeDiff must match a human's own reading of a change, measured against the hand-authored
ground-truth mappings in `src/test/data/diffs/`:

* **90% of test cases with zero mismatched bytes.**
* **99% of test cases with at most 1% of bytes mismatched.**

Neither is met yet. 792 of the 2,001 fixtures carry a hand-painted ground truth - every byte of
both files labelled with what a human says happened to it - and CodeDiff's own highlighting is
compared against it byte by byte, under each of its two highlighting presets (`--full`, which keeps
brackets, separators and leading whitespace, and `--minimal`, which drops them):

| preset | zero mismatched bytes | at most 1% mismatched | mismatched bytes, whole corpus |
|---|---|---|---|
| `--full` | **528 (66.7%)** | **674 (85.1%)** | 27,094 of 26.3M (0.10%) |
| `--minimal` | **559 (70.6%)** | **702 (88.6%)** | 16,524 of 26.3M (0.06%) |

The whole-corpus rate is far below 1% because most of each file is unchanged and nobody gets that
wrong; the per-test-case numbers are the ones that count, since a reader meets the mistakes one
diff at a time. Most of what is left is not in the matching: rendering the human's own node
mapping still disagrees with the painting on 78% of the `--full` bytes and 70% of the `--minimal`
ones, so the highlighting rules own the gap more than the matcher does.

`make update-painting-attribution` measures this and writes one row per fixture and preset to
`research/data/quality/painting_attribution.csv`, which the table is counted from;
`make check-painting-attribution` fails if any fixture gets worse.

# License

Copyright (C) 2026 Marko Ivankovic

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU Affero General Public License as published
by the Free Software Foundation.

See the LICENSE file for the full text of the License.

## Cannot use AGPL software?

A commercial license is available as a monthly subscription through
[GitHub Sponsors](https://github.com/sponsors/ivankovic). It covers internal use of codediff
by your organisation without the source-disclosure obligations of the AGPL. The terms are in
[LICENSE-COMMERCIAL](LICENSE-COMMERCIAL). Pick the tier that names the commercial license as a
benefit. For invoicing or other arrangements, contact me at
[marko@ivankovic.me](mailto:marko@ivankovic.me).

# AI policy

This project uses substantial AI assistance, currently Claude Code. Most commits disclose this
with a `Co-Authored-By` trailer and a link to the session that produced them. This project does not
hide that fact. This project does not treat AI assistance as a lesser way to write software.

Contributions are not accepted at this time, to keep the development speed high (see
[CONTRIBUTING.md](CONTRIBUTING.md)). When that changes, AI-assisted contributions will be as
welcome as any other, disclosed the same way: whoever submits the work is responsible for
understanding it and standing behind it.

# For Developers, human or otherwise

See [CONTRIBUTING.md](CONTRIBUTING.md) for the technology overview, code-quality and testing
expectations, project structure, and what CI checks on every push and PR. `AGENTS.md` has
additional AI-agent-specific conventions.
