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

build: test
	cargo build --release

hermetic-benchmark:
	cargo bench -- diff

hermetic-benchmark-update-baseline:
	cargo bench -- diff --save-baseline baseline

# Benchmark against existing sampled pairs for all languages in benchmark_all.sh (Rust, Python, Go, Kotlin)
# and run analysis afterwards
benchmark-sampled:
	@echo "Running benchmarks for all languages (Rust, Python, Go, Kotlin)..."
	@echo "Results will be written to research/results/"
	cd research && ./measure/benchmark_all.sh --language all --repos-dir /var/tmp/research/small/repositories/ --bin-dir ../target/release
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

# Analysis target that respects current mode
analyze: file-stats

# Mode-specific convenience targets
tiny: override MODE=tiny
tiny: analyze

small: override MODE=small
small: analyze

full: override MODE=full
full: analyze
