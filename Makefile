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

# Fetch repositories using the current mode
fetch: $(LIST) $(SCRIPTS_FETCH_DIR)/dataset.sh
	$(SCRIPTS_FETCH_DIR)/dataset.sh update --root $(REPOSITORIES_DIR) --list $(LIST)

# Analyze file statistics
file-stats: build
	./target/release/file_stats --path $(REPOSITORIES_DIR) --db $(RESEARCH_DIR)/stats.sqlite
	(cd research && uv run ./analysis/file_stats.py)

# Analyze commit statistics
commit-stats: build
	./target/release/commit_stats --path $(REPOSITORIES_DIR) --db $(RESEARCH_DIR)/stats.sqlite

# Analysis target that respects current mode
analyze: file-stats

# Mode-specific convenience targets
tiny: override MODE=tiny
tiny: analyze

small: override MODE=small
small: analyze

full: override MODE=full
full: analyze