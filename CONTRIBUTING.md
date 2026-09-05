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

The Python under `research/analysis/` and `scripts/` has its own unit tests in `research/tests/`,
run by `make test-python` (part of `make test` and of CI's python job). They cover the pure
functions the report scripts are built from; the scripts' shared helpers live in
`research/analysis/_common.py`.

`src/test/` also holds slower, fixture-driven tests, for example `src/test/fixtures/`.
These tests check real diffs against a human-verified ground truth. **These tests must run in
under 5 seconds.**

Semi-automated tests run on the small and full dataset. They take more time to run. Run them when
appropriate, and always before a release.

### Coverage

`make coverage` reports which of this repository's own lines the suite executes. `cargo-llvm-cov`
drives `cargo nextest` directly, so it measures exactly the suite `make test` runs rather than a
second, differently-built one. It writes a browsable report to `target/llvm-cov/html/index.html`
and prints a per-area table, because a single number over 675 files describes nothing:

| area | lines | |
| --- | --- | --- |
| `src/diff/` - the engine | 10761/11118 | 96.8% |
| `src/test/` - fixture helpers | 6845/7329 | 93.4% |
| `src/code/` - parsing, metadata | 899/984 | 91.4% |
| `src/stats/` - sampling, git | 410/477 | 86.0% |
| `src/tui/` - viewer, headless | 5513/6846 | 80.5% |
| `src/bin/` - dev tools | 8066/14205 | 56.8% |
| **product (everything but `src/bin/`)** | **24428/26754** | **91.3%** |
| everything | 33957/42936 | 79.1% |

Measured 2026-09-04 over 1718 tests. The engine and the dev tools are deliberately held to
different standards: `src/bin/` is samplers, benchmark harnesses and `human_solver`, several of
which exist to be run once and read.

**Not a CI gate.** It costs about ten minutes and 5.7GB peak, since it rebuilds the workspace with
instrumentation - and a threshold mostly teaches people to write tests that touch lines. At 96.8%
the engine would never be what tripped a floor; only the dev tools would.

The README badge reads `research/data/coverage/badge.json`, which `make coverage` rewrites.
It is therefore only as current as the last run somebody committed - re-run and commit it when
the number has moved enough to matter.

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
benchmark-quality`. This command diffs every fixture in `src/test/data/diffs/` that has a
human-verified ground truth mapping. It reports how many nodes each fixture gets wrong. Use this
output to see whether a change made diffs better or worse.

The targets (see the README's "Accurate" principle):

* **90% of test cases with zero mismatched visible nodes.**
* **99% of test cases with at most 1% of visible nodes mismatched.**

Both are stated in *visible* nodes - the ones carrying text of their own, per
`codediff::diff::nodes::is_structurally_visible` - not all AST nodes. A wrongly-matched `block` or
`argument_list`, whose every readable byte belongs to a child, is not the same defect as a
wrongly-matched identifier. About 68% of nodes are visible corpus-wide, so the two counts differ.
The benchmark prints both (`Mismatches` / `Vis Mism`), and every clamped `fixtures` test
pins both (`assert_matches_human_mapping_within_limit(name, total, visible)`, which fails if
*either* limit is exceeded).

**Visibility is a property of the tree and the source, never of a diff.** An earlier version
derived it from the renderer - does `diff::text::ranges` emit a span for this node - which made
both the numerator and the denominator move with the algorithm, so a diff that rendered coarsely
had almost nothing it could get visibly wrong. If you are tempted to reintroduce anything
diff-dependent here, see `is_structurally_visible`'s doc comment for the fixture that scored a
perfect zero on 124 real mismatches under the old definition.

### Speed

Automated benchmarks measure the wall-clock time of the main diffing algorithm. These benchmarks
use the Rust criterion library and run over every handmade test case from `src/test/helper.rs`
(`make benchmark-speed`). Run these benchmarks frequently, to catch performance regressions.

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

* `test` - `cargo nextest run --release`, plus `test-mapping-site-js` (a plain-Node test of the
  human-mapping site's vanilla JS, which cargo's suite cannot cover - see the root Makefile).
  Requires `cargo-nextest` (`cargo install cargo-nextest`, one-time). Unlike `cargo test`, nextest
  runs each test in its own process rather than as a thread inside one long-lived binary, so the
  `src/test/helper.rs` fixture caches (never-evicting, process-lifetime) get reclaimed by the OS
  after every test instead of accumulating for the whole suite. Measured on this repo's full suite,
  same machine, both `--release`: peak RSS 10.66GB under plain `cargo test --release` vs 5.37GB
  under `cargo nextest run --release` - about half, at comparable wall-clock time.
* `coverage` - line coverage of the suite over this repository's own code, via `cargo-llvm-cov`
  driving nextest (`cargo install cargo-llvm-cov`, plus `rustup component add llvm-tools-preview`).
  See "Coverage" above for what it reports and why it is not a gate.
* `build` - the `test` target above + `cargo build --release --features stats` (the `stats` feature
  builds the dataset-analysis binaries in `src/bin/`).
* `install` - `cargo install --path . --force`, so `codediff` on `PATH` matches this checkout.
* `install-hooks` - one-time setup that points git at `.githooks/pre-push`, which runs the fast
  subset of what CI checks (`cargo fmt --check`, a per-feature-config `cargo check`, the
  mapping-site JS tests) before a `git push` leaves your machine - see that file's own comment for
  exactly what it does and does not cover. `git push --no-verify` skips it for one push.
* `ci` - the whole of CI, locally: every job in `.github/workflows/ci.yml`, in that file's own
  order. Unlike the pre-push hook above it includes the release build, the full test suite for all
  three feature configs, and the quality gate, so it takes minutes rather than seconds - run it
  when you mean to push, not on every push. It reads the commands out of `ci.yml` itself rather
  than keeping a copy, so it cannot drift from CI; `python3 scripts/ci_local.py --list` shows the
  job ids and `--job <id>` runs one of them. See that script's module docstring for what it can
  and cannot mirror.
Three verbs, and which file a target lives in follows from them:

* **`benchmark-`** measures **codediff**, and lives in the root Makefile. Exactly two, because
  there are exactly two questions: is it right (`benchmark-quality`) and is it fast
  (`benchmark-speed`). Production QA, and neither needs anything a bare checkout lacks.
* **`check-`** gates. Runs in CI on every push and fails the build. `check-quality` gates on
  precisely what `benchmark-quality` measures - the pairing is the point.
* **`measure-`** measures anything that is not codediff alone: other people's tools, or the cloned
  upstream corpus at `REPOSITORIES_DIR`. Lives in `research/Makefile`, never the root one. A number
  that moves when someone else ships a GumTree release is a study of the field, not product QA.

* `benchmark-quality` - runs `benchmark_optimal_solutions`: mismatch count against the
  human-authored ground truth, per fixture (see "Quality" above).
* `benchmark-speed` / `benchmark-speed-update-baseline` - criterion wall-clock of `diff_code` over
  every handmade test case from `src/test/helper.rs` (see "Speed" above). The first compares
  against the saved baseline, the second saves a new one. Note the asymmetry with quality:
  criterion keeps its baseline under `target/`, which is not checked in, so a speed baseline is
  local to one working copy and nothing gates on it.
* `benchmark-ablation` - re-runs `benchmark-quality` with individual solver passes disabled, to see
  what each is worth. A one-off investigation rather than a routine measurement, which is why it is
  not folded into `benchmark-quality`.
* `check-quality` - the gate. What CI runs on every push and what `deploy` runs before it tags a
  release. Fails hard on an accuracy regression against the checked-in baseline; only warns on a
  runtime jump of more than 2x.
* `update-quality-baseline` - re-cuts both baselines after a reviewed change. `deploy` never runs
  it automatically. Note what it does *not* do: the per-fixture accuracy columns are read from the
  `fixtures` stubs, not from the run, so this cannot lower the accuracy bar. Raising a
  limit means editing that fixture's stub - the same file that holds the prose explaining why -
  and `quality_baseline.csv` is then a projection of those limits, pinned by a test.
* `benchmark-speed` / `benchmark-speed-update-baseline` - a criterion wall-clock benchmark of
  `diff_code`, over every handmade test case from `src/test/helper.rs` (see "Speed" above). The
  first compares against the saved baseline, the second saves a new one. Note the asymmetry with
  accuracy: criterion keeps its baseline under `target/`, which is not checked in, so a speed
  baseline is local to one working copy and nothing gates on it.

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
* `cargo build` + `cargo nextest run`, once each for the same three feature configs
* `cargo audit` (checks Cargo.lock against the RustSec advisory database)
* The `human_mapping` site's own vanilla-JS tests (`assets/mapping_site/index.test.js`)

All of these checks must pass before a PR is done. Two things run them locally, before GitHub
does - see "Makefile targets" above:

* `make install-hooks` puts the fast subset (fmt, clippy, the JS tests) on every `git push`, so
  the common mistakes never leave your machine.
* `make ci` runs *all* of the above, driven by parsing `ci.yml` itself so the two cannot drift.
  Minutes, not seconds - it does the release build and full test matrix. What it does not
  reproduce is the runner: it uses your toolchain and OS, where CI gets a clean pinned
  `ubuntu-latest`.
