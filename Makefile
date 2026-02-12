SCRIPTS_FETCH_DIR := ./research/fetch_data

REPOSITORIES_DIR := /var/tmp/research/repositories/
RESEARCH_DIR := /var/tmp/research/

LIST_FULL := ./research/list_of_repositories.csv
LIST_SMALL  := ./research/list_of_repositories_100.csv

clean: clean-db
	rm -rf $(REPOSITORIES_DIR)

clean-db:
	rm -rf $(RESEARCH_DIR)/stats.sqlite

test:
	cargo test

build: test
	cargo build --release

analysis: file-stats

small-fetch-repositories: $(LIST_SMALL) $(SCRIPTS_FETCH_DIR)/dataset.sh
	$(SCRIPTS_FETCH_DIR)/dataset.sh update --list $(LIST_SMALL)

fetch-repositories: $(LIST_FULL) $(SCRIPTS_FETCH_DIR)/dataset.sh
	$(SCRIPTS_FETCH_DIR)/dataset.sh update --list $(LIST_FULL)

file-stats: build
	./target/release/file_stats --path $(REPOSITORIES_DIR) --db $(RESEARCH_DIR)/stats.sqlite
	(cd research && uv run ./analysis/file_stats.py)

commit-stats: build
	./target/release/commit_stats --path $(REPOSITORIES_DIR) --db $(RESEARCH_DIR)/stats.sqlite

