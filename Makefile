SCRIPTS_FETCH_DIR := ./research/fetch_data

LIST_FULL := ./research/list_of_repositories.csv
LIST_SMALL  := ./research/list_of_repositories_100.csv
LIST_TINY  := ./research/list_of_repositories_tiny.csv

# Default mode is "tiny", can be overridden with "make MODE=small" or "make MODE=full"
MODE ?= tiny

# Resolve the appropriate list based on mode
LIST := $(if $(filter tiny,$(MODE)),$(LIST_TINY),$(if $(filter small,$(MODE)),$(LIST_SMALL),$(LIST_FULL)))

# Resolve directories based on mode
REPOSITORIES_DIR := /var/tmp/research/$(MODE)/repositories/
RESEARCH_DIR := /var/tmp/research/$(MODE)/

clean: clean-db
	rm -rf $(REPOSITORIES_DIR)

clean-db:
	rm -rf $(RESEARCH_DIR)/stats.sqlite

test:
	cargo test

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

# Tags the current commit as v<Cargo.toml version> and pushes the tag, which triggers
# .github/workflows/release.yml to build codediff for Linux/macOS/Windows and attach the
# binaries to a new GitHub Release. Requires a clean working tree and HEAD to already match
# origin/main, so the tag can't silently point at uncommitted or unpushed work that the release
# workflow (running against what GitHub already has) would never actually see.
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
	$(eval VERSION := $(shell grep -m1 '^version = ' Cargo.toml | sed -E 's/version = "(.*)"/\1/'))
	@echo "Tagging and pushing v$(VERSION)..."
	git tag v$(VERSION)
	git push origin v$(VERSION)

hermetic-benchmark:
	cargo bench --bench diff_code_benchmark

hermetic-benchmark-update-baseline:
	cargo bench --bench diff_code_benchmark -- --save-baseline baseline

# Benchmark against existing sampled pairs for all languages in benchmark_all.sh (Rust, Python, Go, Kotlin)
# and run analysis afterwards
benchmark-sampled:
	@echo "Running benchmarks for all languages (Rust, Python, Go, Kotlin)..."
	@echo "Results will be written to research/results/"
	cd research && ./measure/benchmark_all.sh --language all --repos-dir /var/tmp/research/small/repositories/ --bin-dir ../target/release
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
