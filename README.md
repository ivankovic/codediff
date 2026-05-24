# CodeDiff

Fast, robust, syntax aware code diffing.

# Guiding principles

## Robust

CodeDiff must be able to process 100% of all commits in the full test dataset.

The full test dataset contains the git commit history, as available on the main branch, for about
7400 open source git repositories. The list of repositories was extracted from the Gentoo Linux
distribution and is available in list_of_repositories.csv.

A smaller list of 100 repositories, called "small" dataset is available for faster iterations when
debugging.

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

SQLite is used to store user configuation and other runtime data.

The UI is a Terminal UI written using the excellent Ratatui and Crossterm libraries.

### UI design patterns

The UI uses the [Component architecture](https://ratatui.rs/concepts/application-patterns/component-architecture/).

Each component encapsulates its own state, event handlers, and rendering logic.

## Code quality

Code must always be formatted using the automated standard Rust formatter.

No Rust check errors are allowed. Rust check should be run frequently.

## Testing

Automated tests should be run frequently during coding.

Benchmarks should be used to measure quality. These should be run on demand.

### Automated tests

Each file in src/ should end with the test module for that file, as is typicall in Rust. These tests
should test both happy-path and corner cases.

**Tests in src/ must run in under 1 second**.

Tests in src/ should use the src/tests/helper.rs to get handmade high quality test data. These tests
should cover happy-path tests and corner cases extensively.

**Tests in tests/ must run in under 5 seconds.**

Semi-automated tests that run on the small and full dataset that take some time to run and should be
run when appropriate, definitely before any release.

### How should tests handle dependencies?

*No mocks*. Mocks prevent testing through the interface and are brittle.

Ideally, the real implementation is used.

When necessary, e.g. for filesystem or database access, fake in-memory implementations should be used.

### Benchmarks

Automated benchmarks that use the Rust criterion library should measure the wall clock time for the
main algorithm, the code diff, on all handmade test cases provided by src/test/helper.rs must be run
frequently to ensure no regressions.

## Code structure

Rust's project structure must be followed.

Some directories don't exist yet but should be created if the need arises.

<root of the repository>
    |- /src             <- The implementation
        |- main.rs      <- The main entry point, spawns the background threads and the UI
        |- code.rs      <- The struct and methods related to reading and parsing one unit of code
        |- code/        <- Sensible implementation units related to code.rs
        |- diff.rs      <- Everything related to actually diffing two or more units of code
        |- diff/        <- Sensible implementation units related to diff.rs
        |- stats.rs     <- Tools used to process large datasets to guide the design
        |- stast/       <- Sensible implementation units related to stats.rs
        |- app.rs       <- The app controler, responds to events and controlls the UI
        |- tui/         <- All TUI components go in this directory
            |- SPECS.md <- TUI specs
        |- tui.rs       <- The visual elements of the TUI, the view
    |- /test            <- Integration and end-to-end automated tests
    |- /benches         <- Benchmarks
    |- README.md        <- This file. Only very high level information goes here
    |- AGENTS.md        <- AI-only instructions
    |- SPECS.md         <- Detailed specifications and all decisions that were taken
    |- REVIEW.md        <- Comments about the codebase that need to be improved uppon
    |- TODO.md          <- List of small to  mid size TODO items that need to be fixed in the future

The SPECS.md and README.md files can exist in any subdirectory, and they always serve the same
purpose:

*  README.md - High level summary. Must be readable to humans.
*  SPECS.md - Semi-structured collection of specifications and a decision log of every decision that
   was taken during implementation.

The TODO.md and REVIEW.md files are always only in the root of the repository.
