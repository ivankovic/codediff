SCRIPTS_FETCH_DIR := ./research/fetch_data

LIST_FULL := ./research/list_of_repositories.csv
LIST_SMALL  := ./research/list_of_repositories_100.csv
LIST_TINY  := ./research/list_of_repositories_tiny.csv

# Default mode is "tiny", can be overridden with "make MODE=small" or "make MODE=full"
MODE ?= tiny

# Resolve the appropriate list based on mode. Deliberately `=` (recursive/lazy), not `:=`
# (immediate): `tiny`/`small`/`full` below set MODE as a target-specific variable (`override
# MODE=...`), which only reaches these via lazy re-expansion at the point each recipe line
# actually uses them - `:=` would instead bake in whatever MODE was at Makefile-parse time
# (always the global `tiny` default) and silently ignore the override.
LIST = $(if $(filter tiny,$(MODE)),$(LIST_TINY),$(if $(filter small,$(MODE)),$(LIST_SMALL),$(LIST_FULL)))

# Resolve directories based on mode - see the `LIST` comment above for why `=`, not `:=`.
REPOSITORIES_DIR = /var/tmp/research/$(MODE)/repositories/
RESEARCH_DIR = /var/tmp/research/$(MODE)/

clean: clean-db
	rm -rf $(REPOSITORIES_DIR)

clean-db:
	rm -rf $(RESEARCH_DIR)/stats.sqlite

test:
	cargo test

# Opens a test fixture's before/after files side by side in nvim. Usage: make view-diff NAME=rust-add-if
view-diff:
	@if [ -z "$(NAME)" ]; then \
		echo "usage: make view-diff NAME=<fixture-name>  (e.g. rust-add-if - see src/test/data/diffs/)" >&2; \
		exit 1; \
	fi
	./src/test/view_test_diff.sh $(NAME)

# --features stats: every target below this one (file-stats, commit-stats, sample-pairs,
# benchmark-pairs, and the language-specific variants) runs a stats-gated binary
# (file_stats/commit_stats/sample_code_pairs/benchmark_diff_pairs) that doesn't exist in
# target/release without it - see Cargo.toml's `stats` feature.
build: test
	cargo build --release --features stats

# Scores codediff's diffing accuracy against the human-authored ground truth corpus in
# src/test/data/ - the project's own primary regression gate for any change to the diff
# algorithm (see TODO.md). --features test-fixtures: this binary needs codediff::test's
# fixture-loading helpers, gated separately from `stats` since it needs no git2/rusqlite.
benchmark-optimal:
	cargo run --release --features test-fixtures --bin benchmark_optimal_solutions

# Same benchmark, but with --csv (so matching-reasons-report below has something to read) and a
# summary of which algorithm pass (ASTMappingReason) is responsible for how much of the diff.
benchmark-optimal-report:
	cargo run --release --features test-fixtures --bin benchmark_optimal_solutions -- --csv
	(cd research && uv run ./analysis/matching_reasons_report.py)

# Compares codediff against other diff tools (Unix diff, GumTree) on line-level agreement with
# the human-authored mapping, plus runtime. --features test-fixtures: benchmark_other, like
# benchmark_optimal_solutions, needs codediff::test's fixture-loading helpers. Requires GUMTREE_BIN
# to point at a built GumTree distribution's bin/gumtree - unlike everything else in this Makefile,
# this is an external, non-Rust tool dependency this target can't provide on its own (see
# research/drivers/gumtree-batch/build.sh for the optional warm-JVM timing variant).
benchmark-other:
	cargo run --release --features test-fixtures --bin benchmark_other -- --csv
	(cd research && uv run ./analysis/benchmark_other_report.py)

# Leave-one-out ablation study over the diff algorithm's optional heuristic passes - see
# research/measure/ablation_study.sh's own header comment for exactly what it measures.
# Usage: make ablation-study [OUT_DIR=path]  (default OUT_DIR: research/ablation)
ablation-study:
	./research/measure/ablation_study.sh $(OUT_DIR)

QUALITY_BASELINE := research/quality_baseline.txt
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

# Tags the current commit as v<Cargo.toml version> and pushes the tag, which triggers
# .github/workflows/release.yml to build codediff for Linux/macOS/Windows and attach the
# binaries to a new GitHub Release. Requires a clean working tree and HEAD to already match
# origin/main (so the tag can't silently point at uncommitted or unpushed work that the release
# workflow, running against what GitHub already has, would never actually see) and requires
# check-quality to pass first.
deploy:
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
	$(eval VERSION := $(shell grep -m1 '^version = ' Cargo.toml | sed -E 's/version = "(.*)"/\1/'))
	@echo "Tagging and pushing v$(VERSION)..."
	git tag v$(VERSION)
	git push origin v$(VERSION)

hermetic-benchmark:
	cargo bench --bench diff_code_benchmark

hermetic-benchmark-update-baseline:
	cargo bench --bench diff_code_benchmark -- --save-baseline baseline

# Benchmark against existing sampled pairs for the four primary languages (Rust, Python, Go,
# Kotlin) and run analysis afterwards. Uses benchmark_all_extended.sh restricted to these four via
# --language - the only difference from benchmark-sampled-extended below was ever which languages
# ran by default, not the underlying script.
benchmark-sampled:
	@echo "Running benchmarks for Rust, Python, Go, Kotlin..."
	@echo "Results will be written to research/results/"
	cd research && ./measure/benchmark_all_extended.sh --language "Rust Python Go Kotlin" --repos-dir /var/tmp/research/small/repositories/ --bin-dir ../target/release
	@echo ""
	@echo "Running analysis..."
	cd research && uv run ./analysis/benchmark_report.py

# Benchmark with extended language set (every language with a tree-sitter grammar - see
# ALL_LANGUAGES in benchmark_all_extended.sh) and higher node limit
benchmark-sampled-extended:
	@echo "Running extended benchmarks for all supported languages..."
	@echo "Results will be written to research/results/"
	@echo "Using 20000 node limit, max 100 commits per repo"
	cd research && ./measure/benchmark_all_extended.sh \
		--language all \
		--repos-dir /var/tmp/research/small/repositories/ \
		--bin-dir ../target/release \
		--limit 20000 \
		--max-commits 100 \
		--timeout-min 120 \
		--continue-on-error
	@echo ""
	@echo "Running analysis..."
	cd research && uv run ./analysis/benchmark_report.py

# Fetch repositories using the current mode
fetch: $(LIST) $(SCRIPTS_FETCH_DIR)/dataset.sh
	$(SCRIPTS_FETCH_DIR)/dataset.sh update --root $(REPOSITORIES_DIR) --list $(LIST)

# Analyze file statistics
file-stats: build
	./target/release/file_stats --path $(REPOSITORIES_DIR) --db $(RESEARCH_DIR)/stats.sqlite
	(cd research && uv run ./analysis/file_stats.py $(RESEARCH_DIR)/stats.sqlite)

# Analyze commit statistics
commit-stats: build
	./target/release/commit_stats --path $(REPOSITORIES_DIR) --db $(RESEARCH_DIR)/stats.sqlite
	(cd research && uv run ./analysis/commit_stats.py $(RESEARCH_DIR)/stats.sqlite)

# Ad-hoc file_stats/commit_stats run over one specific directory (not the dataset-mode pipeline
# above) - useful for debugging those binaries without the fetch/mode machinery. No `build`
# prerequisite: research/measure/debug.sh already does its own `cargo build --release --features
# stats`, so adding one here would just force a redundant full test+build first.
# Usage: make debug-stats DIR=/path/to/repos [DEBUG_MODE=dirs|all|repositories]
DEBUG_MODE ?= dirs
debug-stats:
	@if [ -z "$(DIR)" ]; then \
		echo "usage: make debug-stats DIR=/path/to/repos [DEBUG_MODE=dirs|all|repositories]" >&2; \
		exit 1; \
	fi
	./research/measure/debug.sh --$(DEBUG_MODE) $(DIR)

# Sample real (repository, commit, path) code pairs for benchmark test data, per language.
sample-pairs: build
	./target/release/sample_code_pairs --path $(REPOSITORIES_DIR) --output research/sampled_code_pairs.csv

# Measure diff_code's speed, memory, AST size and mapping operation count across a sampled CSV.
benchmark-pairs: build
	./target/release/benchmark_diff_pairs --csv research/sampled_code_pairs.csv --repo-root $(REPOSITORIES_DIR) --output research/diff_pairs_benchmark.csv

# Rust-only variants of the two targets above, fixed at 1000 sampled pairs (sample_code_pairs'
# default --count and --seed, so re-running against the same checkouts reproduces the same pairs).
# This is the pipeline used to track diff_code's performance over time on real Rust commits;
# re-run benchmark-pairs-rust after any change to the diff algorithm to measure its effect.
sample-pairs-rust: build
	./target/release/sample_code_pairs --path $(REPOSITORIES_DIR) --output research/sampled_code_pairs_rust.csv --language Rust

benchmark-pairs-rust: build
	./target/release/benchmark_diff_pairs --csv research/sampled_code_pairs_rust.csv --repo-root $(REPOSITORIES_DIR) --output research/diff_pairs_benchmark_rust.csv

# Size/LOC-changed statistics and distribution plots for sample-pairs-rust's output. Depends on
# sample-pairs-rust having already been run (needs the checked-out repositories, not just the CSV).
code-pair-diff-stats:
	(cd research && uv run ./analysis/code_pair_diff_stats.py sampled_code_pairs_rust.csv --repo-root $(REPOSITORIES_DIR) --output-csv code_pair_diff_stats_rust.csv)

# Compares two benchmark-pairs-rust runs (e.g. before/after a diff_code algorithm change) and
# charts the difference. Usage: make benchmark-pairs-diff BEFORE=path/to/before.csv AFTER=path/to/after.csv
# (save a copy of research/diff_pairs_benchmark_rust.csv before re-running benchmark-pairs-rust,
# then pass that copy as BEFORE and the fresh run as AFTER).
benchmark-pairs-diff:
	@if [ -z "$(BEFORE)" ] || [ -z "$(AFTER)" ]; then \
		echo "usage: make benchmark-pairs-diff BEFORE=path/to/before.csv AFTER=path/to/after.csv" >&2; \
		exit 1; \
	fi
	(cd research && uv run ./analysis/diff_pairs_benchmark_comparison.py --before $(abspath $(BEFORE)) --after $(abspath $(AFTER)))

# Extended language targets with 20000 node limit
sample-pairs-java: build
	./target/release/sample_code_pairs --path $(REPOSITORIES_DIR) --output research/sampled_code_pairs_java.csv --language Java --count 1000 --max-commits-per-repo 100

benchmark-pairs-java: build
	./target/release/benchmark_diff_pairs --csv research/sampled_code_pairs_java.csv --repo-root $(REPOSITORIES_DIR) --output research/benchmark_java.csv --max-combined-nodes 20000

sample-pairs-javascript: build
	./target/release/sample_code_pairs --path $(REPOSITORIES_DIR) --output research/sampled_code_pairs_javascript.csv --language JavaScript --count 1000 --max-commits-per-repo 100

benchmark-pairs-javascript: build
	./target/release/benchmark_diff_pairs --csv research/sampled_code_pairs_javascript.csv --repo-root $(REPOSITORIES_DIR) --output research/benchmark_javascript.csv --max-combined-nodes 20000

sample-pairs-typescript: build
	./target/release/sample_code_pairs --path $(REPOSITORIES_DIR) --output research/sampled_code_pairs_typescript.csv --language TypeScript --count 1000 --max-commits-per-repo 100

benchmark-pairs-typescript: build
	./target/release/benchmark_diff_pairs --csv research/sampled_code_pairs_typescript.csv --repo-root $(REPOSITORIES_DIR) --output research/benchmark_typescript.csv --max-combined-nodes 20000

# Analysis target that respects current mode
analyze: file-stats

# Mode-specific convenience targets
tiny: override MODE=tiny
tiny: analyze

small: override MODE=small
small: analyze

full: override MODE=full
full: analyze
