# CodeDiff

Fast, robust, syntax aware code diffing.

# Installation

```
cargo install codediff
```

This builds from source, so you'll need a C compiler on `PATH` alongside a Rust toolchain
(edition 2024, so rustc 1.85+) - the bundled SQLite and the tree-sitter grammars are all compiled
from C during the build. The first `cargo install` will take a couple of minutes because of this
plus the `lto = "fat"` release profile.

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

# For Developers, human or otherwise

This part of the README is mostly used to tell the AI how to work in this code. Still, useful for
humans too.

## Technology

The project is completely written in Rust.

User configuration (e.g. the active theme) is stored on disk via `confy`. SQLite is used
separately, by the dataset-analysis tools in `src/bin/`, to store the stats they collect.

The UI is a Terminal UI written using the excellent Ratatui and Crossterm libraries.

### UI design patterns

The UI uses the [Component architecture](https://ratatui.rs/concepts/application-patterns/component-architecture/).

Each component encapsulates its own state, event handlers, and rendering logic.

## Code quality

Code must always be formatted using the automated standard Rust formatter.

No Rust check errors are allowed. Rust check should be run frequently.

## Testing

Automated tests should be run frequently during coding.

Diff quality and diff speed are measured separately (see "Quality" and "Speed" below) and should
be checked on demand, definitely before any release.

### Automated tests

Each file in src/ should end with the test module for that file, as is typical in Rust. These tests
should test both happy-path and corner cases, and should use src/test/helper.rs to get handmade
high quality test data.

**Per-file unit tests must run in under 1 second.**

src/test/ additionally holds slower, fixture-driven tests (e.g. src/test/optimal_solutions/, which
checks real diffs against human-verified ground truth). **These must run in under 5 seconds.**

Semi-automated tests that run on the small and full dataset take some time to run and should be run
when appropriate, definitely before any release.

### How should tests handle dependencies?

*No mocks*. Mocks prevent testing through the interface and are brittle.

Ideally, the real implementation is used.

When necessary, e.g. for filesystem or database access, fake in-memory implementations should be used.

### Quality

`cargo run --release --bin benchmark_optimal_solutions` diffs every fixture in
src/test/data/diffs/ that has a human-verified ground truth mapping, and reports how many nodes
each one gets wrong. Use this to check whether a change made diffs better or worse.

### Speed

Automated benchmarks, using the Rust criterion library, measure the wall clock time of the main
diffing algorithm on all handmade test cases from src/test/helper.rs. Run these frequently to
catch performance regressions.

## Code structure

Rust's project structure must be followed.

Some directories don't exist yet but should be created if the need arises.

```
<root of the repository>
    |- /src             <- The implementation
        |- main.rs      <- Entry point: parses CLI args and starts the TUI
        |- code.rs      <- The struct and methods related to reading and parsing one unit of code
        |- code/        <- Sensible implementation units related to code.rs
        |- diff.rs      <- Everything related to actually diffing two or more units of code
        |- diff/        <- Sensible implementation units related to diff.rs
        |- stats.rs     <- Tools used to process large datasets to guide the design
        |- stats/       <- Sensible implementation units related to stats.rs
        |- tui.rs       <- Declares the TUI's submodules and sets up logging
        |- tui/         <- The TUI itself: app.rs (controller), ui.rs (terminal rendering),
        |                  components/, widgets/
        |   |- SPECS.md <- TUI specs
        |- test/        <- Shared test helpers, plus slower fixture-driven tests (see "Testing")
        |- bin/         <- Standalone developer tools (benchmarking, dataset sampling, etc.)
    |- /benches         <- Benchmarks
    |- /research        <- Datasets and analysis scripts used to guide design decisions
    |- README.md        <- This file. Only very high level information goes here
    |- AGENTS.md        <- AI-only instructions
    |- REVIEW.md        <- Comments about the codebase that need to be improved upon
    |- TODO.md          <- List of small to mid size TODO items that need to be fixed in the future
```

The SPECS.md and README.md files can exist in any subdirectory, and they always serve the same
purpose:

*  README.md - High level summary. Must be readable to humans.
*  SPECS.md - Semi-structured collection of specifications and a decision log of every decision that
   was taken during implementation.

TODO.md and REVIEW.md are normally root-only. A subsystem can have its own TODO.md for issues
specific to it (e.g. src/diff/TODO.md) - but REVIEW.md stays root-only.
