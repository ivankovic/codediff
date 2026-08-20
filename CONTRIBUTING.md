# Contributing

Thank you for considering a contribution, human or AI-assisted (see the README's AI policy). This
file gives practical information about how to work in this codebase. `AGENTS.md` adds more
conventions for AI agents, mostly style rules for the TUI code.

## Technology

The project is completely written in Rust.

CodeDiff stores user configuration, for example the active theme, on disk with `confy`. The
dataset-analysis tools in `src/bin/` use a separate SQLite database to store the stats that they
collect.

The UI is a terminal UI, written with the Ratatui and Crossterm libraries.

### UI design patterns

The UI uses the [Component architecture](https://ratatui.rs/concepts/application-patterns/component-architecture/).

Each component encapsulates its own state, event handlers, and rendering logic.

## Code quality

Format all code with `cargo fmt`, the standard Rust formatter. CI enforces this on every push and
pull request.

No Rust check errors are allowed. Run `cargo clippy` frequently. CI also enforces `cargo clippy`,
across all three Cargo feature configs (see "CI" below).

## Testing

Run automated tests frequently during coding.

Diff quality and diff speed are measured separately (see "Quality" and "Speed" below). Check both
on demand, and always before a release. `make deploy` already gates on quality automatically (see
`make check-quality` in the Makefile).

### Automated tests

Each file in `src/` ends with its own test module, as is typical in Rust. These tests must cover
both the happy path and corner cases. Use `src/test/helper.rs` to get handmade, high-quality test
data.

**Per-file unit tests must run in under 1 second.**

`src/test/` also holds slower, fixture-driven tests, for example `src/test/optimal_solutions/`.
These tests check real diffs against a human-verified ground truth. **These tests must run in
under 5 seconds.**

Semi-automated tests run on the small and full dataset. They take more time to run. Run them when
appropriate, and always before a release.

### Test dependencies

**No mocks.** Mocks block testing through the interface, and mocks are brittle.

Use the real implementation where possible.

Where the real implementation is not possible, for example for filesystem or database access, use
a fake in-memory implementation.

Install `jj` (`cargo install --root /var/tmp/tools jj-cli`, keeping it out of the system-wide
cargo bin directory) if you touch `src/jj_configure.rs`. That module's claims about how jj invokes
a diff tool - directory trees by default, file pairs with extensions preserved under
`diff-invocation-mode = "file-by-file"` - were verified empirically against jj 0.44.0, and should
be re-verified the same way rather than assumed: jj has renamed its config surface before.

### Quality

Run `cargo run --release --features test-fixtures --bin benchmark_optimal_solutions`, or `make
benchmark-optimal`. This command diffs every fixture in `src/test/data/diffs/` that has a
human-verified ground truth mapping. It reports how many nodes each fixture gets wrong. Use this
output to see whether a change made diffs better or worse.

The targets (see the README's "Accurate" principle):

* **90% of test cases with zero mismatched visible nodes.**
* **99% of test cases with at most 4% of visible nodes mismatched.**

Both are stated in *visible* nodes - the ones the renderer emits a span for, per
`codediff::diff::text::visible_node_ids` - not all AST nodes. A wrongly-matched `block` or
`argument_list` the reader never sees on its own is not the same defect as a wrongly-matched
identifier, and only about 3% of nodes are visible corpus-wide, so the two counts differ a lot. The
benchmark prints both (`Mismatches` / `Vis Mism`), and every clamped `optimal_solutions` test pins
both (`assert_matches_human_mapping_within_limit(name, total, visible)`, which fails if *either*
limit is exceeded).

Note what the 4% bar means at this corpus's scale: the median fixture has ~130 visible nodes, so 4%
of it is about 5 nodes - a median fixture clears the second bar with up to five visible mismatches.
Only the smallest fixtures (under 25 visible nodes, 50 of 468) are still tight enough that a single
visible mismatch breaks it.

### Speed

Automated benchmarks measure the wall-clock time of the main diffing algorithm. These benchmarks
use the Rust criterion library and run over every handmade test case from `src/test/helper.rs`
(`make hermetic-benchmark`). Run these benchmarks frequently, to catch performance regressions.

## Code structure

Follow Rust's standard project structure.

Some directories in the list below do not exist yet. Create them if the need arises.

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
        |- bin/         <- Standalone developer tools: benchmarking, dataset sampling, and more
    |- /benches         <- Benchmarks
    |- /research        <- Datasets and analysis scripts used to guide design decisions
    |- README.md        <- High-level project summary. Must be readable to humans.
    |- CONTRIBUTING.md  <- This file
    |- AGENTS.md        <- AI-only instructions
    |- REVIEW.md        <- Comments about the codebase that need to be improved upon
    |- TODO.md          <- List of small to mid size TODO items that need to be fixed in the future
```

`SPECS.md` and `README.md` files can exist in any subdirectory. They always serve the same purpose
in every location:

* `README.md` — a high-level summary. It must be readable by humans.
* `SPECS.md` — a semi-structured collection of specifications, plus a decision log of every
  decision made during implementation.

`TODO.md` and `REVIEW.md` are normally root-only. A subsystem can have its own `TODO.md` for issues
specific to that subsystem, for example `src/diff/TODO.md`. `REVIEW.md` stays root-only.

## Makefile targets

These are the repository-root Makefile's targets - product concerns only: build, test, install, the
quality gate, release. The corpus, measurement, analysis and paper targets live in
`research/Makefile`, are run from that directory (`cd research && make <target>`), and are
documented there.

### Build, test, quality

* `test` - `cargo test --release`, plus `test-mapping-site-js` (a plain-Node test of the
  human-mapping site's vanilla JS, which cargo's suite cannot cover - see the root Makefile).
* `build` - `cargo test` + `cargo build --release --features stats` (the `stats` feature builds the
  dataset-analysis binaries in `src/bin/`).
* `install` - `cargo install --path . --force`, so `codediff` on `PATH` matches this checkout.
* `install-hooks` - one-time setup that points git at `.githooks/pre-push`, which runs the fast
  subset of what CI checks (`cargo fmt --check`, a per-feature-config `cargo check`, the
  mapping-site JS tests) before a `git push` leaves your machine - see that file's own comment for
  exactly what it does and does not cover. `git push --no-verify` skips it for one push.
* `benchmark-optimal` - runs `benchmark_optimal_solutions`, the project's primary diff-quality
  gate. This measures mismatch count against the human-authored ground truth (see "Quality" above).
* `check-quality` - what `deploy` runs before it tags a release. This target gates on a checked-in
  quality baseline. It fails hard on an accuracy regression. It only warns, and does not fail, on a
  runtime jump of more than 2x.
* `update-quality-baseline` - deliberately lowers that bar, after a reviewed improvement. `deploy`
  never runs this target automatically.
* `hermetic-benchmark` / `hermetic-benchmark-update-baseline` - a criterion wall-clock benchmark of
  `diff_code`, over every handmade test case from `src/test/helper.rs` (see "Speed" above). The
  first command compares against the saved baseline. The second command saves a new baseline.

### Release

* `deploy` - publishes a release everywhere: `deploy-crates` then `deploy-github`, in that order
  (crates.io first, since a publish there can never be undone - only yanked - while a GitHub tag
  and Release are trivial to redo). Both refuse to run on a dirty working tree, on a `HEAD` that
  does not match `origin/main`, or on a `check-quality` regression (`deploy-checks`, shared by
  both - a plain `make deploy` only pays for it once).
* `deploy-crates` - publishes the current `Cargo.toml` version to crates.io (`cargo publish
  --locked`). Requires `cargo login` to already be configured locally (or `CARGO_REGISTRY_TOKEN`
  set).
* `deploy-github` - tags the current commit `v<Cargo.toml version>` and pushes the tag. This push
  triggers `.github/workflows/release.yml`, which builds and publishes the cross-platform
  `codediff` binaries as a GitHub Release.

## CI

Every push and pull request runs (see `.github/workflows/ci.yml`):

* `cargo fmt --check`
* `cargo clippy --tests -- -D warnings`, once each for the three Cargo feature configs (default,
  `test-fixtures`, `stats` - see Cargo.toml's `[features]`)
* `cargo build` + `cargo test`, once each for the same three feature configs
* `cargo audit` (checks Cargo.lock against the RustSec advisory database)
* The `human_mapping` site's own vanilla-JS tests (`assets/mapping_site/index.test.js`)

All of these checks must pass before a PR is done. `make install-hooks` runs the fast subset of
these (fmt, clippy, the JS tests) locally before every `git push`, so most failures show up before
CI does - see "Makefile targets" above.
