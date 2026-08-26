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
	cargo test --release

# Plain-Node regression test for the human_mapping site's own vanilla JS (assets/mapping_site/) -
# no npm dependency, no build step, matching that directory's own convention (see index.js's header
# comment). Cargo's test suite can't cover this: it's browser-side JS embedded verbatim via
# include_str! into generate_mapping_site.rs, never executed by anything Rust runs.
test-mapping-site-js:
	node assets/mapping_site/index.test.js

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

QUALITY_BASELINE := research/data/quality/quality_baseline.txt
BENCH_OUTPUT := target/benchmark_optimal_output.txt

# Runs benchmark_optimal_solutions and gates on it against $(QUALITY_BASELINE) - invoked by
# `deploy` before it ever tags/pushes. Two numbers, two different treatments:
# - TOTAL_MISMATCHES (the accuracy score vs. the human-authored ground truth) is algorithm-only,
#   not machine-dependent, so it's safe to hard-fail on: deploy stops if this got worse.
# - MS_PER_FIXTURE (wall-clock runtime) varies too much machine-to-machine to gate on an absolute
#   number reliably - only warns (doesn't fail) if it's jumped more than 2x, as a loose sanity
#   check for a real, gross regression rather than ordinary noise.
# Run `make update-quality-baseline` after a deliberate, reviewed improvement to lower the bar -
# the baseline is never updated automatically as a side effect of a normal deploy.
check-quality:
	cargo run --release --features test-fixtures --bin benchmark_optimal_solutions | tee $(BENCH_OUTPUT)
	@total=$$(grep -m1 '^TOTAL' $(BENCH_OUTPUT) | awk '{print $$2}'); \
	baseline_total=$$(grep '^TOTAL_MISMATCHES=' $(QUALITY_BASELINE) | cut -d= -f2); \
	ms=$$(grep -oE '[0-9.]+ms/fixture' $(BENCH_OUTPUT) | grep -oE '[0-9.]+'); \
	baseline_ms=$$(grep '^MS_PER_FIXTURE=' $(QUALITY_BASELINE) | cut -d= -f2); \
	echo ""; \
	echo "Quality: TOTAL mismatches = $$total (baseline: $$baseline_total)"; \
	echo "Runtime: $$ms ms/fixture (baseline: $$baseline_ms)"; \
	if [ "$$total" -gt "$$baseline_total" ]; then \
		echo "error: quality regressed - TOTAL mismatches $$total > baseline $$baseline_total" >&2; \
		exit 1; \
	fi; \
	over_2x=$$(awk -v ms="$$ms" -v base="$$baseline_ms" 'BEGIN { print (ms > base * 2) ? 1 : 0 }'); \
	if [ "$$over_2x" = "1" ]; then \
		echo "warning: runtime is more than 2x the baseline ($$ms ms/fixture vs $$baseline_ms ms/fixture) - investigate before deploying" >&2; \
	fi

# Updates $(QUALITY_BASELINE) to the numbers from a fresh check-quality run - a deliberate,
# separate step, not something `deploy` ever does on its own.
update-quality-baseline: check-quality
	@total=$$(grep -m1 '^TOTAL' $(BENCH_OUTPUT) | awk '{print $$2}'); \
	ms=$$(grep -oE '[0-9.]+ms/fixture' $(BENCH_OUTPUT) | grep -oE '[0-9.]+'); \
	{ \
		echo "# Baseline compared against by \`make check-quality\` / \`make deploy\` - see Makefile."; \
		echo "# TOTAL_MISMATCHES: benchmark_optimal_solutions' first TOTAL row (mismatches vs. the"; \
		echo "# human-authored ground truth in src/test/data/diffs/) - a hard gate, since this is"; \
		echo "# algorithm-only and not machine-dependent: deploy fails if this number goes up."; \
		echo "# MS_PER_FIXTURE: benchmark_optimal_solutions' own \"Runtime: ... ms/fixture\" line -"; \
		echo "# informational only (a >2x jump warns but doesn't fail deploy), since wall-clock time"; \
		echo "# varies by machine."; \
		echo "# Update deliberately via \`make update-quality-baseline\` after a reviewed improvement,"; \
		echo "# not automatically as a side effect of every deploy."; \
		echo "TOTAL_MISMATCHES=$$total"; \
		echo "MS_PER_FIXTURE=$$ms"; \
	} > $(QUALITY_BASELINE); \
	echo "Updated $(QUALITY_BASELINE): TOTAL_MISMATCHES=$$total MS_PER_FIXTURE=$$ms"

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
