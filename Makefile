# Product-side targets only: build, test, install, the quality gate, and release.
#
# Everything that exists to produce the papers and empirical studies - corpus fetching, sampling,
# measurement, analysis, paper builds - lives in research/Makefile instead. Run those from there:
#
#     cd research && make <target>          # e.g. rq1-report, introductory-paper, file-stats
#
# The split is deliberate: this file should stay readable to someone working on codediff itself,
# who has no reason to care about the research corpus. `benchmark-optimal` and `check-quality`
# stay here despite reading/writing files under research/data/, because they are this project's
# own regression gate and release gate, not research artifacts.

test: test-mapping-site-js
	cargo nextest run --release

# Plain-Node regression tests for the human_mapping site's own vanilla JS (assets/mapping_site/) -
# no npm dependency, no build step, matching that directory's own convention (see index.js's header
# comment). Cargo's test suite can't cover this: it's browser-side JS embedded verbatim via
# include_str! into generate_mapping_site.rs, never executed by anything Rust runs.
#
# Both files, not just index.test.js: viewer.js is by far the larger of the two scripts and went
# uncovered until 2026-08-27. It is mostly DOM wiring, which these do not fake - what they cover is
# the logic underneath, including the `kind:occurrence` node path that has to agree with
# `helper::path_for_node` on the Rust side (the two pin each other through a shared example).
test-mapping-site-js:
	node assets/mapping_site/index.test.js
	node assets/mapping_site/viewer.test.js

# --features stats: every target below this one (file-stats, commit-stats, sample-pairs,
# benchmark-pairs, and the language-specific variants) runs a stats-gated binary
# (file_stats/commit_stats/sample_code_pairs/benchmark_diff_pairs) that doesn't exist in
# target/release without it - see Cargo.toml's `stats` feature.
build: test
	cargo build --release --features stats

# Installs codediff from this checkout onto PATH (~/.cargo/bin by default), so `codediff` and any
# `git difftool`/`git diff` config pointing at it matches this working tree - including
# uncommitted changes, since `cargo install --path .` builds from whatever's on disk, not HEAD -
# instead of whatever was last installed. `--force` overwrites an existing install rather than
# erroring, since the whole point of this target is "make PATH match what's here now". No `test`/
# `build` prerequisite: `cargo install` does its own release build already, so depending on
# either would just force a redundant one first.
install:
	cargo install --path . --force

# Points git at the checked-in .githooks/ directory (not the default, untracked .git/hooks/), so
# `git push` runs the fast subset of what CI checks (cargo fmt --check, a per-feature-config
# `cargo check`, the mapping-site JS tests - see .githooks/pre-push's own comment for why it's a
# subset, not a full CI mirror) before the push leaves your machine. One-time, per clone - git
# does not do this automatically just because .githooks/ exists in the repo.
install-hooks:
	git config core.hooksPath .githooks
	@echo "hooks enabled (git config core.hooksPath .githooks):"
	@echo "  pre-commit - regenerates src/test/data/diffs.csv when a commit touches the fixture"
	@echo "               corpus, so the checked-in inventory never goes stale (.githooks/pre-commit)"
	@echo "  pre-push   - fmt + clippy + site JS tests, the fast subset of CI (.githooks/pre-push)"

# Scores codediff's diffing accuracy against the human-authored ground truth corpus in
# src/test/data/ - the project's own primary regression gate for any change to the diff
# algorithm (see TODO.md). --features test-fixtures: this binary needs codediff::test's
# fixture-loading helpers, gated separately from `stats` since it needs no git2/rusqlite.
benchmark-optimal:
	cargo run --release --features test-fixtures --bin benchmark_optimal_solutions -- --csv

# Regenerates src/test/data/diffs.csv: one row per fixture with its provenance, size, and how far
# each of its two ground truths has been taken. Cheap and fully derived from the corpus, so re-run
# it after adding fixtures, finishing a tree mapping, or painting text ranges - the file is
# checked in so the inventory is readable without running anything, not because it is authored.
diff-inventory:
	cargo run --release --features test-fixtures --bin diff_inventory

# Lints the analysis scripts under research/. Lives here rather than in research/Makefile so that
# `make lint` covers everything CI lints from one place; the rule set itself is pinned in
# research/pyproject.toml (see that file for why it is pinned rather than left on ruff's defaults).
#
# Two passes, mirroring the shape the Rust side already has (`cargo fmt --check` and clippy as
# separate CI jobs): the formatter decides layout, the linter decides everything else. Width is set
# to 100 in research/pyproject.toml to match what these scripts were already written to.
lint-python:
	ruff check research
	ruff format --check research

QUALITY_BASELINE := research/data/quality/quality_baseline.csv
RUNTIME_BASELINE := research/data/quality/quality_baseline.txt
BENCH_OUTPUT := target/benchmark_optimal_output.txt

# The release gate: `deploy` runs this before it ever tags or publishes.
#
# The accuracy half is **per fixture**, not one aggregate number, and that distinction is the whole
# design - see benchmark_optimal_solutions.rs's own quality-gate section for the measurements
# behind it. In short: this corpus grows deliberately toward hard cases, so any aggregate (a total,
# or a rate) reads "we added 35 hard fixtures" as "the algorithm regressed", and the old gate did
# exactly that. Comparing fixture by fixture, with fixtures that have no baseline row exempt, asks
# the only question that survives new data - did anything that already had a baseline get worse?
#
# The runtime half stays a warning rather than a gate: wall-clock varies too much machine-to-machine
# to fail on (278.8 and 324.9 ms/fixture on the same machine, days apart), so a >2x jump is flagged
# as a loose check for a gross regression and nothing more.
#
# Run `make update-quality-baseline` after a deliberate, reviewed change to move the bar - never
# automatically as a side effect of a deploy.
#
# `SHELL`/`.SHELLFLAGS` are overridden for this target alone so that `pipefail` is available: the
# gate's verdict is the benchmark's exit status, and without it the `| tee` would hand make `tee`'s
# status instead - a red gate that reports success, which is the one failure a gate must not have.
# (`/bin/sh` is dash on Debian/Ubuntu and has no `pipefail`, so this cannot just be `set -o`.)
check-quality: SHELL := /bin/bash
check-quality: .SHELLFLAGS := -o pipefail -c
check-quality:
	cargo run --release --features test-fixtures --bin benchmark_optimal_solutions -- \
		--compare $(QUALITY_BASELINE) | tee $(BENCH_OUTPUT)
	@ms=$$(grep -oE '[0-9.]+ms/fixture' $(BENCH_OUTPUT) | grep -oE '[0-9.]+'); \
	baseline_ms=$$(grep '^MS_PER_FIXTURE=' $(RUNTIME_BASELINE) | cut -d= -f2); \
	echo ""; \
	echo "Runtime: $$ms ms/fixture (baseline: $$baseline_ms)"; \
	over_2x=$$(awk -v ms="$$ms" -v base="$$baseline_ms" 'BEGIN { print (ms > base * 2) ? 1 : 0 }'); \
	if [ "$$over_2x" = "1" ]; then \
		echo "warning: runtime is more than 2x the baseline ($$ms ms/fixture vs $$baseline_ms ms/fixture) - investigate before deploying" >&2; \
	fi

# Rewrites both baselines from a fresh run - a deliberate, separate step, never something `deploy`
# does on its own.
#
# Deliberately does NOT depend on check-quality, and deliberately does not gate: the moment you
# most need this is right after a *reviewed* regression (a net-positive trade that costs one
# fixture), and a target that refused to run while the gate was red would be useless exactly then.
# Run `make check-quality` first and read which fixtures moved - that reading is the review, and
# there is no way to automate it.
update-quality-baseline:
	cargo run --release --features test-fixtures --bin benchmark_optimal_solutions -- \
		--write-baseline $(QUALITY_BASELINE) | tee $(BENCH_OUTPUT)
	@ms=$$(grep -oE '[0-9.]+ms/fixture' $(BENCH_OUTPUT) | grep -oE '[0-9.]+'); \
	{ \
		echo "# Runtime baseline for \`make check-quality\` - see Makefile."; \
		echo "#"; \
		echo "# MS_PER_FIXTURE: benchmark_optimal_solutions' own \"Runtime: ... ms/fixture\" line."; \
		echo "# Informational only: a >2x jump warns, it never fails a deploy, because wall-clock"; \
		echo "# time varies by machine far more than any real regression would."; \
		echo "#"; \
		echo "# The accuracy gate does NOT live here. It is per-fixture, in quality_baseline.csv"; \
		echo "# beside this file, because no single number over a corpus that keeps gaining hard"; \
		echo "# fixtures can tell a real regression apart from new data - measured, see the"; \
		echo "# quality-gate section in src/bin/benchmark_optimal_solutions.rs."; \
		echo "MS_PER_FIXTURE=$$ms"; \
	} > $(RUNTIME_BASELINE); \
	echo "Updated $(QUALITY_BASELINE) and $(RUNTIME_BASELINE) (MS_PER_FIXTURE=$$ms)"

# Shared preconditions for deploy-github/deploy-crates, not meant to be run directly. Requires a
# clean working tree and HEAD to already match origin/main (so a tag/publish can't silently point
# at uncommitted or unpushed work that GitHub's release workflow, and anyone installing from
# crates.io, would never actually see) and requires check-quality to pass first. Both
# deploy-github and deploy-crates depend on this as a normal prerequisite (not via a nested
# `$(MAKE)` call) specifically so that a single `make deploy` only pays for it once - Make only
# remakes a given prerequisite once per invocation, however many targets depend on it - while
# `make deploy-github` or `make deploy-crates` alone (e.g. retrying just one half after it failed)
# still gets the same safety net on its own.
deploy-checks:
	@if [ -n "$$(git status --porcelain)" ]; then \
		echo "error: working tree is dirty - commit or stash before deploying" >&2; \
		exit 1; \
	fi
	git fetch origin main
	@if [ "$$(git rev-parse HEAD)" != "$$(git rev-parse origin/main)" ]; then \
		echo "error: HEAD does not match origin/main - push your commits first" >&2; \
		exit 1; \
	fi
	$(MAKE) check-quality

# Publishes the current Cargo.toml version to crates.io. `--locked` refuses to publish if
# Cargo.lock and Cargo.toml have drifted apart, so the published crate's dependency resolution is
# exactly what check-quality (via deploy-checks) actually ran against, not a fresh resolution
# computed at publish time. Requires `cargo login` to already be configured locally (or
# CARGO_REGISTRY_TOKEN set) - same "use whatever credentials are already there" approach
# deploy-github takes for `git push`.
deploy-crates: deploy-checks
	cargo publish --locked

# Tags the current commit as v<Cargo.toml version> and pushes the tag, which triggers
# .github/workflows/release.yml to build codediff for Linux/macOS/Windows and attach the
# binaries to a new GitHub Release.
deploy-github: deploy-checks
	$(eval VERSION := $(shell grep -m1 '^version = ' Cargo.toml | sed -E 's/version = "(.*)"/\1/'))
	@echo "Tagging and pushing v$(VERSION)..."
	git tag v$(VERSION)
	git push origin v$(VERSION)

# Publishes a release everywhere. crates.io first, GitHub second: a bad Cargo.toml or a
# crates.io-side hiccup is better caught before anything public-facing exists on GitHub yet (a git
# tag and a Release are trivial to create after the fact; a crates.io publish for a given version
# can never be undone, only yanked). Prerequisite order is what enforces this, not just intent -
# `make` runs a target's prerequisites in the order listed, one fully at a time, unless invoked
# with `-j`.
deploy: deploy-crates deploy-github

hermetic-benchmark:
	cargo bench --bench diff_code_benchmark

hermetic-benchmark-update-baseline:
	cargo bench --bench diff_code_benchmark -- --save-baseline baseline
