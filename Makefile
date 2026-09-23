# Product-side targets only: build, test, install, the benchmarks, the quality gate, and release.
#
# **Three verbs, and the split between them is what this file's boundary is made of:**
#
#   benchmark-   measures codediff, and only codediff. benchmark-quality answers both questions a
#                change raises - is it right, and is it fast - in one run. Production QA rather
#                than a study, and it needs nothing a bare checkout lacks: it runs against
#                src/test/data/, which ships with the repo.
#   check-       gates. Runs in CI on every push and fails the build. Today that is check-quality
#                alone, gating on precisely what benchmark-quality measures - the pairing is the
#                point. Deliberately not in .githooks/pre-push - see that file for why a slow hook
#                is worse than no hook.
#   measure-     measures anything that is not codediff alone: other people's diff tools, or the
#                cloned upstream corpus at REPOSITORIES_DIR. Lives in research/Makefile, never
#                here. A number that moves when someone else ships a GumTree release is a study of
#                the field, not product QA - so the tool comparisons are `measure-tools-*` over
#                there even though they read this repository's own fixtures.
#
# Everything else that exists to produce the papers and empirical studies - corpus fetching,
# sampling, analysis, paper builds - lives in research/Makefile too. Run those from there:
#
#     cd research && make <target>          # e.g. apted-budget-report, introductory-paper, measure-file-stats
#
# The split is deliberate: this file should stay readable to someone working on codediff itself,
# who has no reason to care about the research corpus. `benchmark-quality` and `check-quality`
# write under research/data/ anyway, because that is where this project keeps measurements - but
# producing them is product QA, and neither reads anything research/ produces. Nothing here
# invokes a research/ target.

# Which of this repository's own lines the test suite actually executes.
#
# Not one of the three verbs above, and deliberately so: `benchmark-` and `check-` measure
# codediff's *output* against a corpus, and `measure-` studies the field. This measures the test
# suite instead - a fact about how well this repository is examining itself, not about how well
# codediff diffs. It is on demand rather than a gate: a coverage threshold in CI mostly teaches
# people to write tests that touch lines, and the number that matters here (`src/diff/`) is
# already high enough that a floor would only ever fire on the dev tools.
#
# One feature set for every local release-binary target, so alternating `make build`,
# `make check-quality` and `make diff-inventory` does not re-link the fat-LTO binary each time:
# cargo fingerprints by feature set, and `stats` is a superset of `test-fixtures`. CI's quality
# job overrides this with the smaller set (see ci.yml) so its cache stays free of git2/rusqlite.
FEATURES ?= stats

# Usage: make benchmark-ablation [OUT_DIR=path]
OUT_DIR ?= research/data/ablation

.PHONY: coverage test test-mapping-site-js test-python build install install-hooks benchmark-quality \
	diff-inventory lint-python ci benchmark-ablation check-quality update-quality-baseline \
	check-painting-attribution update-painting-attribution diff-gif \
	deploy-checks deploy-crates deploy-github deploy

# `cargo-llvm-cov` drives `cargo nextest` directly, so this runs exactly the suite `make test`
# does - `--all-features`, not `--features $(FEATURES)`. Those were the same thing until `test`
# moved to all features; `stats` (the FEATURES default) does not pull in `web`, so `src/web/` was
# compiled out of the measurement entirely and 53 of its tests never ran. Around ten minutes and
# 6GB peak: it rebuilds the whole workspace with instrumentation, which is why it is not wired
# into anything that runs often.
#
# Writes a browsable report to target/llvm-cov/html/index.html and prints a per-area summary -
# see scripts/coverage_report.py for why per-area rather than llvm-cov's own per-file table.
coverage:
	# Explicit, because `--no-report` deliberately does *not* clean: it exists so several test
	# invocations can accumulate into one report. Without this the numbers only ever climb, since
	# each run adds to whatever the last one left behind.
	cargo llvm-cov clean --workspace
	# `--no-fail-fast`: nextest otherwise stops at the first failure, which on a ten-minute
	# instrumented run means throwing the run away to learn about one test.
	cargo llvm-cov nextest --no-report --release --all-features --no-fail-fast
	cargo llvm-cov report --release --html
	cargo llvm-cov report --release --json --summary-only \
	  | python3 scripts/coverage_report.py --badge research/data/coverage/badge.json
	@echo
	@echo "Browsable report: target/llvm-cov/html/index.html"
	@echo "README badge: commit research/data/coverage/badge.json to publish this number"

# Every test in the repository, in one pass: the three JS/Python suites above, then the whole Rust
# suite under `--all-features`.
#
# `--all-features`, not the default set and not `--features $(FEATURES)`. Four of this crate's
# features gate their own tests (see Cargo.toml's [features]), and the default set is `tui` alone,
# so a bare `cargo nextest run` silently skips 460 of them - every human_solver test, every
# generate_mapping_site test, the analyzer's. Measured 2026-09-22: default 3968 tests,
# test-fixtures 4339, stats 4375, web 4014, all-features 4428. That last number is a strict
# superset of the union of the other four (4421), the extra 7 being tests that need two features
# at once, e.g. generate_showcase's (test-fixtures + web).
#
# **This is not a substitute for `make ci`.** CI builds and tests each feature *separately*
# (ci.yml's matrix: "", test-fixtures, stats, web) precisely to prove each one compiles on its
# own, which one --all-features pass cannot show. CI also runs the clippy matrix and the two
# baseline gates (check-quality, check-painting-attribution), neither of which is a test. Use this
# to run everything quickly; use `make ci` before pushing.
#
# Release, deliberately: the fixture corpus is 3209 of these tests and runs real diffs. Under the
# default debug profile that is minutes of overflow-checked APTED rather than ~90 seconds.
test: test-mapping-site-js test-web-js test-python
	cargo nextest run --release --all-features

# The pure functions under research/analysis/ and scripts/ (CSV readers, LaTeX number format, LOC
# buckets, CI-matrix expansion, ...) - see research/tests/. Runs in research/'s own uv
# environment, which is where pytest is a dev dependency.
test-python:
	cd research && uv run pytest -q

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

# The same for the browser viewer's own logic (assets/web/model.js - cursor, change navigation,
# search, overlay painting, ported from the TUI's widgets and pinned to them test by test). Embedded
# via include_str! into src/web/server.rs and never executed by anything Rust runs, so this is its
# only coverage; app.js (DOM wiring) has none, like the mapping site's.
test-web-js:
	node assets/web/model.test.js

# And for the GitHub Pages showcase's shim (assets/showcase/showcase.js - the URL <-> selection
# mapping and the table that answers the viewer's /api/ calls from baked JSON). Embedded via
# include_str! into src/bin/generate_showcase.rs, never executed by anything Rust runs.
test-showcase-js:
	node assets/showcase/showcase.test.js

# $(FEATURES) defaults to `stats` because every research target that depends on this one
# (measure-file-stats, measure-commit-stats, sample-pairs, measure-pairs, and the language-specific
# variants) runs a stats-gated binary (file_stats/commit_stats/sample_code_pairs/
# benchmark_diff_pairs) that doesn't exist in target/release without it - see Cargo.toml's `stats`
# feature. Deliberately not dependent on `test`: those research targets are measurement runs, and
# `deploy-checks` gates on `check-quality` explicitly.
build:
	cargo build --release --features $(FEATURES)

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
	@echo "  pre-commit - formats the Rust and Python a commit stages (cargo fmt / ruff format),"
	@echo "               re-staging only files with no further unstaged changes, and regenerates"
	@echo "               src/test/data/diffs.csv when a commit touches the fixture corpus, so the"
	@echo "               checked-in inventory never goes stale (.githooks/pre-commit)"
	@echo "  pre-push   - fmt + clippy + site JS tests, the fast subset of CI (.githooks/pre-push)"

# Scores codediff's diffing accuracy against the human-authored ground truth corpus in
# src/test/data/ - the project's own primary regression gate for any change to the diff
# algorithm (see TODO.md). Named to pair with `check-quality`, which gates on exactly this
# measurement: benchmark- produces the number, check- fails the build on it.
#
# The binary needs only `test-fixtures` (codediff::test's fixture-loading helpers), which is
# what CI passes; locally it runs under $(FEATURES) so it shares `make build`'s target/release.
benchmark-quality:
	$(BENCH_QUALITY) --csv

# Regenerates src/test/data/diffs.csv: one row per fixture with its provenance, size, and how far
# each of its two ground truths has been taken. Cheap and fully derived from the corpus, so re-run
# it after adding fixtures, finishing a tree mapping, or painting text ranges - the file is
# checked in so the inventory is readable without running anything, not because it is authored.
diff-inventory:
	cargo run --release --features $(FEATURES) --bin diff_inventory

# Records assets/diff-vs-codediff.gif: the README's animation of what a syntax-aware diff buys you,
# on the showcase's "Replace two loops with built-ins" case. A bar sweeps across the viewer and
# back, swapping GNU `diff`'s whole-line marks for codediff's mapping on the same unmoved code.
#
# Three steps, and each is somebody else's source of truth: `generate_showcase` bakes the case both
# ways (it already does, for the Pages showcase - this target adds no new notion of "as diff marks
# it"), `scripts/diff_gif_segments.js` runs the browser viewer's own model.js over each bake to get
# the coloured runs, and `scripts/record_diff_gif.py` draws them. Nothing here decides what to
# paint, which is the point: a second painter would drift from the product it advertises.
#
# The GIF is committed, unlike everything `generate_showcase` produces, because a README image has
# to resolve in a plain checkout and on crates.io. Re-run this after anything that changes how the
# viewer paints, and commit the result.
#
# Needs `web` on top of `test-fixtures` (the showcase generator serialises web payloads), Node for
# the extractor, and research/'s uv environment for Pillow.
DIFF_GIF_CASE ?= python-refactoring
DIFF_GIF_OUT ?= assets/diff-vs-codediff.gif
diff-gif:
	@tmp=$$(mktemp -d) && trap 'rm -rf "$$tmp"' EXIT; \
	cargo run --release --features test-fixtures,web --bin generate_showcase -- --out "$$tmp" >/dev/null && \
	cd research && uv run python ../scripts/record_diff_gif.py \
		--showcase "$$tmp" --case $(DIFF_GIF_CASE) --out ../$(DIFF_GIF_OUT)

# Lints every Python file in the repository: the analysis scripts under research/, the CI mirror
# and coverage report under scripts/, and the bdiff driver under assets/. One target so that this,
# the pre-push hook and CI cannot lint three different subsets; the rule set is pinned in the root
# ruff.toml (see that file for why it is pinned rather than left on ruff's defaults).
#
# Two passes, mirroring the shape the Rust side already has (`cargo fmt --check` and clippy as
# separate CI jobs): the formatter decides layout, the linter decides everything else.
#
# `ruff` is the one tool in this file that neither a bare checkout nor the Rust toolchain brings:
# CI installs its own pinned copy (`astral-sh/ruff-action`) and `nix develop` has it in the
# devShell, so a plain shell is the one setup nothing covers, and the bare "ruff: command not
# found" it used to fail with named no way out.
lint-python:
	@command -v ruff >/dev/null 2>&1 || { \
		echo "make lint-python needs \`ruff\` on PATH, which CI installs for itself." >&2; \
		echo "Install it for every repository under your user, at the version ci.yml pins:" >&2; \
		echo "    uv tool install ruff@$(RUFF_VERSION)" >&2; \
		echo "(or work inside \`nix develop\`, whose devShell already has it)" >&2; \
		exit 1; \
	}
	ruff check $(PYTHON_DIRS)
	ruff format --check $(PYTHON_DIRS)

# The ruff version .github/workflows/ci.yml pins its `astral-sh/ruff-action` steps to. Only the
# message above reads it; nothing here installs or enforces a version, because the lint that
# decides a push is CI's copy and not this machine's - which is exactly why the message has to name
# CI's version and not just "ruff". A hand-written copy of a number that lives in another file, so
# `test_the_ruff_version_named_outside_ci_matches_the_one_ci_pins` (research/tests/) fails the build
# if this and CONTRIBUTING.md stop agreeing with ci.yml. The command list in ci_local.py is read out
# of ci.yml rather than copied for the same reason; a version behind an action's `with:` has no
# such reader.
RUFF_VERSION := 0.16.4

PYTHON_DIRS := research scripts assets

# Runs everything .github/workflows/ci.yml runs, here, before the push rather than after it.
#
# Distinct from `install-hooks`' pre-push hook, which is deliberately the fast subset (fmt, clippy,
# the JS tests) and stays that way: this one includes the release-profile build and full test suite
# for all three feature configs and the quality gate, so it is minutes, not seconds - a thing you
# run when you mean to push, not on every push.
#
# It reads the job list and every command out of ci.yml itself rather than repeating them here, so
# it cannot drift from CI the way a hand-copied list would - see scripts/ci_local.py's own module
# docstring for what it can and cannot mirror (short version: the commands, yes; the clean pinned
# ubuntu-latest runner, no). `python3 scripts/ci_local.py --list` shows the jobs and
# `--job <id>` runs one.
ci:
	python3 scripts/ci_local.py

# Re-runs `benchmark-quality` with individual solver passes disabled, to see what each is worth.
# A codediff measurement over this repository's own fixtures, so it lives here rather than in
# research/ despite having been written there.
#
# Usage: make benchmark-ablation [OUT_DIR=path]  (default: research/data/ablation; set above)
benchmark-ablation:
	./scripts/ablation_study.sh $(OUT_DIR)

QUALITY_BASELINE := research/data/quality/quality_baseline.csv
RUNTIME_BASELINE := research/data/quality/quality_baseline.txt
BENCH_OUTPUT := target/benchmark_optimal_output.txt
# The one invocation behind benchmark-quality, check-quality and update-quality-baseline; they
# differ only in the flag they pass it.
BENCH_QUALITY := cargo run --release --features $(FEATURES) --bin benchmark_optimal_solutions --
# The "Runtime: N ms/fixture" figure out of $(BENCH_OUTPUT), as a number.
extract-ms = grep -oE '[0-9.]+ms/fixture' $(BENCH_OUTPUT) | grep -oE '[0-9.]+'

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
#
# The `mkdir -p` is what makes that `tee` safe on a machine that has never built this project.
# Both sides of a pipeline start at once, so `tee` opens `$(BENCH_OUTPUT)` immediately - it does
# not wait for `cargo` to create `target/` first, and loses that race on a clean checkout. With
# `pipefail` that is a hard failure of the gate, and it is invisible locally, where `target/`
# always exists: it only fires in CI, and only when the Rust cache misses (evicted after 7 days
# of no pushes), which is exactly when nobody is expecting an infrastructure failure. Observed
# for real - `tee: target/benchmark_optimal_output.txt: No such file or directory`, while cargo
# was still downloading crates.
check-quality: SHELL := /bin/bash
check-quality: .SHELLFLAGS := -o pipefail -c
check-quality:
	@mkdir -p $(dir $(BENCH_OUTPUT))
	$(BENCH_QUALITY) --compare $(QUALITY_BASELINE) | tee $(BENCH_OUTPUT)
	@ms=$$($(extract-ms)); \
	baseline_ms=$$(grep '^MS_PER_FIXTURE=' $(RUNTIME_BASELINE) | cut -d= -f2); \
	echo ""; \
	echo "Runtime: $$ms ms/fixture (baseline: $$baseline_ms)"; \
	over_2x=$$(awk -v ms="$$ms" -v base="$$baseline_ms" 'BEGIN { print (ms > base * 2) ? 1 : 0 }'); \
	if [ "$$over_2x" = "1" ]; then \
		echo "warning: runtime is more than 2x the baseline ($$ms ms/fixture vs $$baseline_ms ms/fixture) - investigate before deploying" >&2; \
	fi

# The painting gate, and its baseline.
#
# `research/data/quality/painting_attribution.csv` holds one row per (fixture, preset): how many
# bytes of the hand-painted ground truth codediff's rendering disagrees with (`real_bytes`), how
# many of those survive rendering the *human* mapping instead (`renderer_bytes` - the residue no
# matcher improvement can remove), and how far the two renderings differ from each other
# (`matcher_bytes`). The 2026-09-15 census that introduced the split is in
# research/data/quality/painting_failure_census_2026_09_15.md.
#
# Per fixture, never pooled, for the reason `quality_baseline.csv` is per fixture: the painted
# corpus keeps growing, and no single rate over it can tell a real regression from new data. A
# fixture the run has and the baseline does not is new data and passes.
#
# **Unlike `check-quality`, this baseline is a measurement.** That gate's accuracy columns are a
# projection of the hand-authored stub limits, so no run can re-baseline a regression away; here
# there is no hand-authored limit to project, so improving a painting or a mapping moves the number
# legitimately. When that happens, re-run `update-painting-attribution` and say in the commit which
# ground truth changed. A move with no such change is a rendering or matching regression.
check-painting-attribution:
	PAINTING_ATTRIBUTION_CHECK=1 cargo test --release --lib --features test-fixtures \
		painting_failure_census -- --ignored --nocapture

update-painting-attribution:
	cargo test --release --lib --features test-fixtures \
		painting_failure_census -- --ignored --nocapture

# Rewrites both baselines - a deliberate, separate step, never something `deploy` does on its own.
#
# **The accuracy columns do not come from the run.** They are read out of the `optimal_solutions`
# stubs (see `human_mapping::stub_mapping_limits`), so `quality_baseline.csv` is a projection of
# the one hand-authored limit per fixture rather than a second record of the same thing. Only
# `elapsed_ms` and MS_PER_FIXTURE below are measured here. The consequence is the point: this
# command cannot re-baseline an accuracy regression away. Raising a limit means editing the stub,
# which is the file that also holds the prose explaining why - and which no tool rewrites.
#
# Deliberately does NOT depend on check-quality, and deliberately does not gate: the moment you
# most need this is right after a *reviewed* regression (a net-positive trade that costs one
# fixture), and a target that refused to run while the gate was red would be useless exactly then.
# Run `make check-quality` first and read which fixtures moved - that reading is the review, and
# there is no way to automate it.
#
# `mkdir -p` for the same clean-checkout `tee` race described on `check-quality` above, where it
# fails worse rather than louder: this recipe has no `pipefail`, so a failed `tee` would not stop
# it - it would go on to grep an absent $(BENCH_OUTPUT), find no runtime line, and write an empty
# `MS_PER_FIXTURE=` over the runtime baseline.
update-quality-baseline:
	@mkdir -p $(dir $(BENCH_OUTPUT))
	$(BENCH_QUALITY) --write-baseline $(QUALITY_BASELINE) | tee $(BENCH_OUTPUT)
	@ms=$$($(extract-ms)); \
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

