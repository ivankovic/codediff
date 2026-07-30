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

## Makefile targets

A reference for `make <target>`. Most of the dataset/corpus targets accept `MODE=tiny` (default),
`MODE=small`, or `MODE=full`, picking which fetched repository set (see
`research/list_of_repositories*.csv`) to run against; `tiny`/`small`/`full` themselves are
shorthands for "fetch-stats analysis in that mode".

### Build, test, quality

* `test` - `cargo test`.
* `build` - `cargo test` + `cargo build --release --features stats` (needed by every dataset/stats
  target below).
* `view-diff NAME=<fixture>` - opens one `src/test/data/diffs/` fixture's before/after side by
  side in nvim's diff mode.
* `benchmark-optimal` - runs `benchmark_optimal_solutions`, the project's primary diff-quality gate
  (mismatch count vs. the human-authored ground truth - see "Quality" above).
* `benchmark-optimal-report` - same, plus `--csv` and a report on which algorithm pass
  (`ASTMappingReason`) is responsible for how much of the diff.
* `benchmark-other` - compares codediff against Unix `diff` and GumTree on line-level agreement
  with the human mapping, plus runtime. Requires `GUMTREE_BIN` pointing at a built GumTree
  distribution - the one target in this file with an external, non-Rust dependency.
* `ablation-study [OUT_DIR=path]` - leave-one-out study over the diff algorithm's optional
  heuristic passes, measuring each one's real contribution to accuracy on the fixture corpus.
* `check-quality` - what `deploy` runs before it ever tags: gates on `research/quality_baseline.txt`
  (hard-fails on an accuracy regression, warns - doesn't fail - on a >2x runtime jump).
* `update-quality-baseline` - deliberately lowers that bar after a reviewed improvement. Never run
  automatically by `deploy`.
* `hermetic-benchmark` / `hermetic-benchmark-update-baseline` - criterion wall-clock benchmark of
  `diff_code` over every handmade test case from `src/test/helper.rs` (see "Speed" above); save/
  compare against a saved baseline.

### Release

* `deploy` - tags the current commit `v<Cargo.toml version>` and pushes the tag, which triggers
  `.github/workflows/release.yml` to build and publish the cross-platform `codediff` binaries as a
  GitHub Release. Refuses to run on a dirty working tree, a `HEAD` that doesn't match
  `origin/main`, or a `check-quality` regression.

### Dataset / corpus analysis (research/)

* `fetch` - clones/updates the repository set for the current `MODE`.
* `file-stats` / `commit-stats` - run `file_stats`/`commit_stats` over the fetched repositories
  into a SQLite DB, then that binary's own `research/analysis/*.py` report.
* `debug-stats DIR=<path> [DEBUG_MODE=dirs|all|repositories]` - the same two binaries, ad-hoc, over
  one arbitrary directory instead of the fetch/`MODE` pipeline - useful for debugging them
  directly.
* `sample-pairs` / `sample-pairs-rust` / `sample-pairs-java` / `sample-pairs-javascript` /
  `sample-pairs-typescript` - sample real (repository, commit, path) code pairs, per language, for
  benchmark test data.
* `benchmark-pairs` / `benchmark-pairs-rust` / `benchmark-pairs-java` /
  `benchmark-pairs-javascript` / `benchmark-pairs-typescript` - measure `diff_code`'s speed/memory/
  AST size/mapping-operation count across a sampled CSV. `benchmark-pairs-rust` is the one to
  re-run after any diff-algorithm change, to track its effect on real Rust commits.
* `code-pair-diff-stats` - size/LOC-changed statistics and distribution plots for
  `sample-pairs-rust`'s output.
* `benchmark-pairs-diff BEFORE=<csv> AFTER=<csv>` - compares two `benchmark-pairs-rust` runs (e.g.
  before/after a `diff_code` algorithm change) and charts the difference.
* `benchmark-sampled` / `benchmark-sampled-extended` - both run
  `research/measure/benchmark_all_extended.sh` across all sampled pairs, then
  `research/analysis/benchmark_report.py`; the former restricts it to the four primary languages
  (Rust, Python, Go, Kotlin), the latter runs every language with a tree-sitter grammar at a higher
  node limit.
* `analyze` / `tiny` / `small` / `full` - `file-stats`, in the current (or an explicitly
  overridden) `MODE`.
* `clean` / `clean-db` - remove the fetched repositories (and/or just the stats database) for the
  current `MODE`.

## CI

Every push and pull request runs (see `.github/workflows/ci.yml`):

* `cargo fmt --check`
* `cargo build` + `cargo test`, once each for the three Cargo feature configs (default,
  `test-fixtures`, `stats` - see Cargo.toml's `[features]`)
* `cargo audit`, checking Cargo.lock against the RustSec advisory database

All of these must pass before a PR can be considered done.
