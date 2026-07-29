# Contributing

Thanks for considering a contribution - human or AI-assisted (see the README's AI policy). This
file has the practical "how do I work in this codebase" info. `AGENTS.md` supplements it with
additional AI-agent-specific conventions (mostly style rules for the TUI code).

## Technology

The project is completely written in Rust.

User configuration (e.g. the active theme) is stored on disk via `confy`. SQLite is used
separately, by the dataset-analysis tools in `src/bin/`, to store the stats they collect.

The UI is a Terminal UI written using the excellent Ratatui and Crossterm libraries.

### UI design patterns

The UI uses the [Component architecture](https://ratatui.rs/concepts/application-patterns/component-architecture/).

Each component encapsulates its own state, event handlers, and rendering logic.

## Code quality

Code must always be formatted using the automated standard Rust formatter (`cargo fmt` - CI
enforces this on every push/PR).

No Rust check errors are allowed. Rust check should be run frequently (`cargo clippy` - also
enforced by CI, across all three Cargo feature configs - see "CI" below).

## Testing

Automated tests should be run frequently during coding.

Diff quality and diff speed are measured separately (see "Quality" and "Speed" below) and should
be checked on demand, definitely before any release - `make deploy` already gates on quality
automatically (see `make check-quality` in the Makefile).

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

`cargo run --release --features test-fixtures --bin benchmark_optimal_solutions` (or
`make benchmark-optimal`) diffs every fixture in src/test/data/diffs/ that has a human-verified
ground truth mapping, and reports how many nodes each one gets wrong. Use this to check whether a
change made diffs better or worse.

### Speed

Automated benchmarks, using the Rust criterion library, measure the wall clock time of the main
diffing algorithm on all handmade test cases from src/test/helper.rs (`make hermetic-benchmark`).
Run these frequently to catch performance regressions.

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
    |- README.md        <- High-level project summary. Must be readable to humans.
    |- CONTRIBUTING.md  <- This file
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

## CI

Every push and pull request runs (see `.github/workflows/ci.yml`):

* `cargo fmt --check`
* `cargo build` + `cargo test`, once each for the three Cargo feature configs (default,
  `test-fixtures`, `stats` - see Cargo.toml's `[features]`)
* `cargo audit`, checking Cargo.lock against the RustSec advisory database

All of these must pass before a PR can be considered done.
