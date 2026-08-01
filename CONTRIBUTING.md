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

### Quality

Run `cargo run --release --features test-fixtures --bin benchmark_optimal_solutions`, or `make
benchmark-optimal`. This command diffs every fixture in `src/test/data/diffs/` that has a
human-verified ground truth mapping. It reports how many nodes each fixture gets wrong. Use this
output to see whether a change made diffs better or worse.

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

A reference for `make <target>`. Most dataset and corpus targets accept `MODE=tiny` (default),
`MODE=small`, or `MODE=full`. This flag picks the fetched repository set to run against (see
`research/list_of_repositories*.csv`). The targets `tiny`, `small`, and `full` are shorthand for
"run fetch-stats analysis in that mode".

### Build, test, quality

* `test` - `cargo test`.
* `build` - `cargo test` + `cargo build --release --features stats` (needed by every dataset/stats
  target below).
* `view-diff NAME=<fixture>` - opens one `src/test/data/diffs/` fixture's before/after side by
  side in nvim's diff mode.
* `benchmark-optimal` - runs `benchmark_optimal_solutions`, the project's primary diff-quality
  gate. This measures mismatch count against the human-authored ground truth (see "Quality" above).
* `benchmark-optimal-report` - same as `benchmark-optimal`, plus `--csv` output and a report on how
  much of the diff each algorithm pass (`ASTMappingReason`) is responsible for.
* `benchmark-other` - compares codediff against Unix `diff`, GumTree, difftastic, and diffsitter,
  on line-level agreement with the human mapping, plus runtime, then runs `benchmark-other-report`
  below. Each external tool needs its own environment variable pointing at a built binary:
  `GUMTREE_BIN` (a built GumTree distribution - this is the only one with an external, non-Rust
  dependency), `DIFFT_BIN`, and `DIFFSITTER_BIN`. difftastic and diffsitter are plain
  `cargo install`-able. To keep both out of the system-wide cargo bin directory, install them into
  a scratch prefix instead: `cargo install --root /var/tmp/codediff-tools difftastic diffsitter`,
  then
  `export DIFFT_BIN=/var/tmp/codediff-tools/bin/difft DIFFSITTER_BIN=/var/tmp/codediff-tools/bin/diffsitter`.
  All three environment variables are required for a full run - a missing one fails loudly on the
  first fixture in that tool's language scope, rather than silently skipping the tool (see
  `src/bin/benchmark_other.rs`'s own doc comment). This is the slow half of the pair below - a
  fresh GumTree JVM cold-starts once per fixture.
* `benchmark-other-report` - just the analysis/plotting step of `benchmark-other`, over whatever
  `research/benchmark_other.csv` already has on disk. Fast, and needs none of the environment
  variables above, since it never runs the benchmark itself. `introductory-paper` below depends on
  this, not on `benchmark-other`, so rebuilding the paper never pays for a fresh benchmark run.
* `ablation-study [OUT_DIR=path]` - a leave-one-out study over the diff algorithm's optional
  heuristic passes. It measures each pass's real contribution to accuracy on the fixture corpus.
* `check-quality` - what `deploy` runs before it tags a release. This target gates on
  `research/quality_baseline.txt`. It fails hard on an accuracy regression. It only warns, and does
  not fail, on a runtime jump of more than 2x.
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

### Papers

* `introductory-paper` - re-renders the benchmark_other charts and table
  `research/papers/introductory-paper/main.tex` embeds (accuracy chart, runtime chart, and a
  variance table - a generated `.tex` table `\input` directly, not a PNG) from whatever
  `research/benchmark_other.csv` already has on disk, copies them into that paper's `figures/`,
  and rebuilds the PDF with `latexmk`. Deliberately does not depend on `benchmark-other` - run
  that yourself first to refresh the underlying data, this target only re-renders from it, so a
  paper rebuild stays fast. Needs a LaTeX toolchain with the `acmart` class and `cm-super` (see
  that paper's own `README.md` for the install command).
* `introductory-paper-empirical` - re-renders that same paper's empirical-study numbers (Table 1,
  repository/file/language counts, bytes-AST correlation) and its file-types figure, from whatever
  `$(RESEARCH_DIR)/stats.sqlite` already exists for the current `MODE` (pass `MODE=small` or
  `MODE=full` to match whichever `file-stats` run you actually have - see "Dataset / corpus
  analysis" below). These numbers are LaTeX macros in `figures/variables.tex`, generated by
  `research/analysis/file_stats.py`, not hand-transcribed - see that file's own
  `write_paper_variables` doc comment for why (short version: the paper's original Table 1 numbers
  turned out to be hand-copied from a conference slide deck whose own source computation was never
  saved anywhere, and by the time anyone asked why Bytes' max was blank, there was no way to
  answer it). Depends on `file-stats-report`, not `file-stats` - run that yourself first (slow -
  it re-parses every file in the corpus) to (re)populate the mode's `stats.sqlite`.

### Dataset / corpus analysis (research/)

* `fetch` - clones/updates the repository set for the current `MODE`.
* `file-stats` / `commit-stats` - run `file_stats` or `commit_stats` over the fetched repositories,
  into a SQLite database. Then each target runs that binary's own `research/analysis/*.py` report
  (`file-stats-report` is just that report step, over whatever `stats.sqlite` already exists,
  without re-running `file_stats` itself - see `introductory-paper-empirical` above).
* `debug-stats DIR=<path> [DEBUG_MODE=dirs|all|repositories]` - runs the same two binaries, ad hoc,
  over one arbitrary directory, instead of the fetch/`MODE` pipeline. Use this target to debug the
  binaries directly.
* `sample-pairs` / `sample-pairs-rust` / `sample-pairs-java` / `sample-pairs-javascript` /
  `sample-pairs-typescript` - sample real (repository, commit, path) code pairs, per language, for
  benchmark test data.
* `benchmark-pairs` / `benchmark-pairs-rust` / `benchmark-pairs-java` /
  `benchmark-pairs-javascript` / `benchmark-pairs-typescript` - measure `diff_code`'s speed/memory/
  AST size/mapping-operation count across a sampled CSV. `benchmark-pairs-rust` is the one to
  re-run after any diff-algorithm change, to track its effect on real Rust commits.
* `code-pair-diff-stats` - size/LOC-changed statistics and distribution plots for
  `sample-pairs-rust`'s output.
* `benchmark-pairs-diff BEFORE=<csv> AFTER=<csv>` - compares two `benchmark-pairs-rust` runs, for
  example before and after a `diff_code` algorithm change, and charts the difference.
* `benchmark-sampled` / `benchmark-sampled-extended` - both run
  `research/measure/benchmark_all_extended.sh` across all sampled pairs, then
  `research/analysis/benchmark_report.py`. `benchmark-sampled` restricts this to the four primary
  languages: Rust, Python, Go, and Kotlin. `benchmark-sampled-extended` runs every language with a
  tree-sitter grammar, at a higher node limit.
* `analyze` / `tiny` / `small` / `full` - `file-stats`, in the current (or an explicitly
  overridden) `MODE`.
* `clean` / `clean-db` - remove the fetched repositories, or just the stats database, or both, for
  the current `MODE`.

## CI

Every push and pull request runs (see `.github/workflows/ci.yml`):

* `cargo fmt --check`
* `cargo build` + `cargo test`, once each for the three Cargo feature configs (default,
  `test-fixtures`, `stats` - see Cargo.toml's `[features]`)
* `cargo audit` (checks Cargo.lock against the RustSec advisory database)

All of these checks must pass before a PR is done.
