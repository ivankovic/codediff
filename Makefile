# This file is part of the CodeDiff code diffing tool.
#
# Copyright (C) 2026 Marko Ivankovic
#
# This program is free software: you can redistribute it and/or modify
# it under the terms of the GNU Affero General Public License as published
# by the Free Software Foundation, either version 3 of the License, or
# (at your option) any later version.
#
# This program is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
# GNU Affero General Public License for more details.
#
# You should have received a copy of the GNU Affero General Public License
# along with this program. If not, see <https://www.gnu.org/licenses/>.

# Product-side targets: build, test, install, benchmarks, the quality gate, and release.
#
#   benchmark-   measures codediff alone, against src/test/data/ (accuracy and speed in one run).
#   check-       gates that fail CI, each on exactly what a benchmark- target measures. Not in
#                .githooks/pre-push: a slow hook is worse than none.
#   measure-     anything beyond codediff alone (other diff tools, the upstream corpus): lives in
#                research/Makefile, never here.
#
# Corpus fetching, sampling, analysis and paper builds are research/'s too
# (`cd research && make <target>`). Nothing here invokes a research/ target.

# One feature set for every local release-binary target, so alternating targets does not re-link
# the fat-LTO binary (cargo fingerprints by feature set; `stats` is a superset of `test-fixtures`).
# CI's quality job overrides it with the smaller set to keep git2/rusqlite out of its cache.
FEATURES ?= stats

# Usage: make benchmark-ablation [OUT_DIR=path]
OUT_DIR ?= research/data/ablation

.PHONY: coverage test test-rust test-mapping-site-js test-python build install install-hooks \
	benchmark-quality diff-inventory lint-python ci benchmark-ablation check-quality \
	update-quality-baseline check-painting-attribution update-painting-attribution diff-gif \
	readme-screenshot test-web-js test-showcase-js check-versions deploy-checks deploy-crates \
	deploy-github deploy

# Line coverage of the suite `make test` runs (`--all-features`), with a per-area summary (see
# scripts/coverage_report.py). On demand, not a gate: a threshold teaches touching lines. Rebuilds
# everything instrumented - about ten minutes and 6GB - so it runs `test` first: a coverage number
# over a failing suite measures nothing.
coverage: test
	# `--no-report` never cleans, so without this the numbers only ever climb.
	cargo llvm-cov clean --workspace
	# Otherwise nextest stops at the first failure and the instrumented run is wasted.
	cargo llvm-cov nextest --no-report --release --all-features --no-fail-fast
	cargo llvm-cov report --release --html
	cargo llvm-cov report --release --json --summary-only \
	  | python3 scripts/coverage_report.py --badge research/data/coverage/badge.json
	@echo
	@echo "Browsable report: target/llvm-cov/html/index.html"
	@echo "README badge: commit research/data/coverage/badge.json to publish this number"

# Every test, JS, Python and Rust. `--all-features` because several features gate their own tests.
# Not a substitute for `make ci`, which also proves each feature compiles alone and runs the
# clippy matrix and baseline gates. Release, because the fixture tests run real diffs.
test: test-mapping-site-js test-web-js test-python test-rust

# The Rust suite alone, every feature on.
test-rust:
	cargo nextest run --release --all-features

# The unit tests of research/analysis/ and scripts/, in research/'s uv environment.
test-python:
	cd research && uv run pytest -q

# The mapping site's JS (assets/mapping_site/): embedded via include_str!, so Rust never runs it.
# Covers the logic under the DOM wiring, including the node paths that must agree with
# `helper::path_for_node`.
test-mapping-site-js:
	node assets/mapping_site/index.test.js
	node assets/mapping_site/viewer.test.js

# The browser viewer's logic (assets/web/model.js), ported from the TUI and pinned to it test by
# test; its only coverage, since Rust only embeds it.
test-web-js:
	node assets/web/model.test.js

# The GitHub Pages showcase shim (assets/showcase/showcase.js); Rust only embeds it.
test-showcase-js:
	node assets/showcase/showcase.test.js

# $(FEATURES) because the research targets built on this run stats-gated binaries.
build:
	cargo build --release --features $(FEATURES)

# Installs this working tree, uncommitted changes included, over whatever `codediff` is on PATH.
install:
	cargo install --path . --force

# One-time per clone: makes git use the checked-in .githooks/ (the fast subset of CI).
install-hooks:
	git config core.hooksPath .githooks
	@echo "hooks enabled (git config core.hooksPath .githooks):"
	@echo "  pre-commit - formats the Rust and Python a commit stages (cargo fmt / ruff format),"
	@echo "               re-staging only files with no further unstaged changes, and regenerates"
	@echo "               src/test/data/diffs.csv when a commit touches the fixture corpus, so the"
	@echo "               checked-in inventory never goes stale (.githooks/pre-commit)"
	@echo "  pre-push   - fmt + clippy + site JS tests, the fast subset of CI (.githooks/pre-push)"

# Scores diff accuracy and speed against the hand-authored ground truth in src/test/data/.
benchmark-quality:
	$(BENCH_QUALITY) --csv

# Regenerates the checked-in src/test/data/diffs.csv fixture inventory; re-run after changing
# fixtures or their ground truths.
diff-inventory:
	cargo run --release --features $(FEATURES) --bin diff_inventory

# Records the README's assets/diff-vs-codediff.gif from the showcase bake, painted by the viewer's
# own model.js, so it cannot drift from the product. Committed, so it resolves on crates.io; re-run
# after painting changes. Needs Node and research/'s uv environment.
DIFF_GIF_CASE ?= python-refactoring
DIFF_GIF_OUT ?= assets/diff-vs-codediff.gif
diff-gif:
	@tmp=$$(mktemp -d) && trap 'rm -rf "$$tmp"' EXIT; \
	cargo run --release --features test-fixtures,web --bin generate_showcase -- --out "$$tmp" >/dev/null && \
	cd research && uv run python ../scripts/record_diff_gif.py \
		--showcase "$$tmp" --case $(DIFF_GIF_CASE) --out ../$(DIFF_GIF_OUT)

# Records the README's assets/readme-screenshot.png the same way: the viewer drawn by the TUI's
# own widgets into an offscreen terminal (render_tui_screenshot), rasterized by
# scripts/render_tui_screenshot.py in the GIF's font. Committed, so it resolves on crates.io; re-run
# after TUI or painting changes. Needs research/'s uv environment.
README_SCREENSHOT_CASE ?= handmade/python-refactoring
README_SCREENSHOT_OUT ?= assets/readme-screenshot.png
readme-screenshot:
	@tmp=$$(mktemp -d) && trap 'rm -rf "$$tmp"' EXIT; \
	dir=src/test/data/diffs/$(README_SCREENSHOT_CASE) && \
	cargo run --release --features $(FEATURES) --bin render_tui_screenshot -- \
		"$$dir"/before.* "$$dir"/after.* --out "$$tmp/still.json" && \
	cd research && uv run python ../scripts/render_tui_screenshot.py \
		--still "$$tmp/still.json" --out ../$(README_SCREENSHOT_OUT)

# Lints and format-checks all Python (research/, scripts/, assets/) with the rules pinned in
# ruff.toml, the same set the hook and CI lint.
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

# The version ci.yml pins ruff to, only for the message above: CI's copy decides a push. A
# research/tests/ test fails if this or CONTRIBUTING.md disagree with ci.yml.
RUFF_VERSION := 0.16.4

PYTHON_DIRS := research scripts assets

# Runs every command .github/workflows/ci.yml runs, read from ci.yml so it cannot drift; minutes,
# not seconds. `python3 scripts/ci_local.py --list` shows the jobs, `--job <id>` runs one.
ci:
	python3 scripts/ci_local.py

# Re-runs benchmark-quality with individual solver passes disabled, to see what each is worth.
#
# Usage: make benchmark-ablation [OUT_DIR=path]  (default: research/data/ablation)
benchmark-ablation:
	./scripts/ablation_study.sh $(OUT_DIR)

QUALITY_BASELINE := research/data/quality/quality_baseline.csv
RUNTIME_BASELINE := research/data/quality/quality_baseline.txt
BENCH_OUTPUT := target/benchmark_optimal_output.txt
# The one invocation behind benchmark-quality, check-quality and update-quality-baseline.
BENCH_QUALITY := cargo run --release --features $(FEATURES) --bin benchmark_optimal_solutions --
# The "Runtime: N ms/fixture" figure out of $(BENCH_OUTPUT), as a number.
extract-ms = grep -oE '[0-9.]+ms/fixture' $(BENCH_OUTPUT) | grep -oE '[0-9.]+'

# The release gate, run by `deploy`.
#
# Accuracy is compared per fixture, never aggregated: the corpus grows toward hard cases, so any
# aggregate reads new data as a regression. Fixtures without a baseline row pass. Runtime only warns
# (>2x), because wall-clock varies too much between machines to gate on.
#
# bash with `pipefail`, or the `| tee` would report tee's status and a red gate would pass (dash has
# no pipefail). `mkdir -p` first: tee opens its file at once and would lose the race with cargo
# creating target/ on a clean checkout, such as a CI cache miss.
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

# The painting gate and its baseline, per (fixture, preset) in painting_attribution.csv, never
# pooled. Unlike check-quality's, this baseline is a measurement: improving a painting or a mapping
# moves it legitimately, so re-run update-painting-attribution and name the ground truth that changed.
# A move with no such change is a regression.
check-painting-attribution:
	PAINTING_ATTRIBUTION_CHECK=1 cargo test --release --lib --features test-fixtures \
		painting_failure_census -- --ignored --nocapture

update-painting-attribution:
	cargo test --release --lib --features test-fixtures \
		painting_failure_census -- --ignored --nocapture

# Rewrites both baselines; never done by `deploy`. The accuracy columns come from the
# `optimal_solutions` stubs, not the run, so this cannot re-baseline an accuracy regression away.
# Not gated on check-quality, which is red exactly after a reviewed trade-off this exists for.
# `mkdir -p` for check-quality's tee race; without pipefail it would write an empty MS_PER_FIXTURE.
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

# Every packaging recipe names the version Cargo.toml does - see scripts/check_version_sync.py for
# which files repeat it by hand. Part of deploy-checks and of CI's python job.
check-versions:
	python3 scripts/check_version_sync.py

# Shared preconditions for deploy-crates/deploy-github: clean tree, HEAD at origin/main,
# check-versions and check-quality. A prerequisite rather than `$(MAKE)`, so `make deploy` runs it
# once.
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
	$(MAKE) check-versions
	$(MAKE) check-quality

# Publishes to crates.io. `--locked` publishes exactly the resolution check-quality ran against.
deploy-crates: deploy-checks
	cargo publish --locked

# Tags v<version> and pushes it; release.yml then builds and attaches the binaries.
deploy-github: deploy-checks
	$(eval VERSION := $(shell grep -m1 '^version = ' Cargo.toml | sed -E 's/version = "(.*)"/\1/'))
	@echo "Tagging and pushing v$(VERSION)..."
	git tag v$(VERSION)
	git push origin v$(VERSION)

# crates.io first: a publish can never be undone, a tag and a Release can. Prerequisites run in
# order (without -j).
deploy: deploy-crates deploy-github

