# Research-only targets: corpus acquisition, measurement, analysis, and paper builds.
#
# Split out of the repository-root Makefile so that one holds only product concerns (build, test,
# install, the quality gate, release) and this one holds everything that exists to produce the
# papers and empirical studies. Run these from this directory (`cd research && make <target>`).
#
# Paths here are relative to research/, with two deliberate exceptions:
#   * the compiled binaries live in ../target/release (one workspace, one target dir), and
#   * the repository lists live at the repository root, because they describe which projects this
#     project studies and are useful outside research/ (see `LIST_*` below).
#
# Anything needing a release build delegates to the root Makefile's own `build` target rather than
# duplicating the cargo invocation and its `--features stats` requirement.

SCRIPTS_SAMPLING_DIR := ./sampling
BIN_DIR := ../target/release

# Repository lists live at the repository root, not under research/: they enumerate the corpus this
# project studies, which is referenced from outside research/ too.
LIST_FULL := ../list_of_repositories.csv
LIST_SMALL := ../list_of_repositories_100.csv
LIST_TINY := ../list_of_repositories_tiny.csv

# Default mode is "tiny", can be overridden with "make MODE=small" or "make MODE=full"
MODE ?= tiny

# Commits per repository the measure-edit-shape census walks; see the `measure-edit-shape` target.
EDIT_SHAPE_COMMITS ?= 50

# Resolve the appropriate list based on mode. Deliberately `=` (recursive/lazy), not `:=`
# (immediate): `tiny`/`small`/`full` below set MODE as a target-specific variable (`override
# MODE=...`), which only reaches these via lazy re-expansion at the point each recipe line
# actually uses them - `:=` would instead bake in whatever MODE was at Makefile-parse time
# (always the global `tiny` default) and silently ignore the override.
LIST = $(if $(filter tiny,$(MODE)),$(LIST_TINY),$(if $(filter small,$(MODE)),$(LIST_SMALL),$(LIST_FULL)))

# Resolve directories based on mode - see the `LIST` comment above for why `=`, not `:=`.
REPOSITORIES_DIR = /var/tmp/research/$(MODE)/repositories/
RESEARCH_DIR = /var/tmp/research/$(MODE)/

# Delegates to the root Makefile so the cargo invocation (and its `--features stats` requirement)
# lives in exactly one place.
build:
	$(MAKE) -C .. build

clean: clean-db
	rm -rf $(REPOSITORIES_DIR)

clean-db:
	rm -rf $(RESEARCH_DIR)/stats.sqlite

# ── Corpus acquisition ──────────────────────────────────────────────────────────────────────────

# Fetch repositories using the current mode. DEPTH is how many commits back from each branch tip
# to keep; it bounds how far back `sample-pairs-all` can reach, since a sampled (repository,
# commit, path) row is only resolvable while that commit is still in the local clone.
#
# Note that `git fetch --depth=N` *shortens* an existing shallow clone as well as deepening it, so
# re-fetching at a lower DEPTH than a corpus was built with discards history already on disk, and
# silently invalidates any sample drawn against it. Record the depth in that corpus's
# PROVENANCE.md whenever this is run.
DEPTH ?= 1000
fetch: $(LIST) $(SCRIPTS_SAMPLING_DIR)/dataset.sh
	$(SCRIPTS_SAMPLING_DIR)/dataset.sh update --root $(REPOSITORIES_DIR) --list $(LIST) --depth $(DEPTH)

# ── Corpus statistics ───────────────────────────────────────────────────────────────────────────

# Just the analysis/plotting/paper-variables step of measure-file-stats, over whatever
# $(RESEARCH_DIR)/stats.sqlite already has on disk, without re-running file_stats itself. Split
# out for the same reason as timing-report below - see introductory-paper-empirical,
# which depends on this, not on measure-file-stats.
file-stats-report:
	uv run ./analysis/file_stats.py $(RESEARCH_DIR)/stats.sqlite

measure-file-stats: build
	$(BIN_DIR)/file_stats --path $(REPOSITORIES_DIR) --db $(RESEARCH_DIR)/stats.sqlite
	$(MAKE) file-stats-report

measure-commit-stats: build
	$(BIN_DIR)/commit_stats --path $(REPOSITORIES_DIR) --db $(RESEARCH_DIR)/stats.sqlite
	uv run ./analysis/commit_stats.py $(RESEARCH_DIR)/stats.sqlite

# Ad-hoc file_stats/commit_stats run over one specific directory (not the dataset-mode pipeline
# above) - useful for debugging those binaries without the fetch/mode machinery. No `build`
# prerequisite: measure/debug.sh already does its own `cargo build --release --features stats`, so
# adding one here would just force a redundant full test+build first.
# Usage: make debug-stats DIR=/path/to/repos [DEBUG_MODE=dirs|all|repositories]
DEBUG_MODE ?= dirs
debug-stats:
	@if [ -z "$(DIR)" ]; then \
		echo "usage: make debug-stats DIR=/path/to/repos [DEBUG_MODE=dirs|all|repositories]" >&2; \
		exit 1; \
	fi
	./measure/debug.sh --$(DEBUG_MODE) $(DIR)

# Analysis target that respects current mode
analyze: measure-file-stats

# Mode-specific convenience targets
tiny: override MODE=tiny
tiny: analyze

small: override MODE=small
small: analyze

full: override MODE=full
full: analyze

# ── Sampling ────────────────────────────────────────────────────────────────────────────────────

# Sample real (repository, commit, path) code pairs for benchmark test data, per language.
sample-pairs: build
	$(BIN_DIR)/sample_code_pairs --path $(REPOSITORIES_DIR) --output data/samples/sampled_code_pairs.csv

# The languages the RQ1 corpus (data/samples/sampled_code_pairs_*.csv) covers, as
# `<output-file-suffix>:<Language Debug name>` pairs - the two differ in case for most entries, and
# the --language filter matches the Debug name exactly (see `Language` in src/code.rs).
# `r:R` and `scala:Scala` were added 2026-08-21. They were missing while `sample_code_pairs`
# happily sampled both, so 167 of the 3,089 pairs drawn on 2026-08-20 had no per-language CSV and
# were never measured - a silent gap between what the corpus covers and what RQ1 reports on, not
# an error anything surfaced. Keep this list in sync with the languages the sampler actually
# produces (check `sampled_code_pairs_all.csv`'s own `language` column, not this list).
RQ1_LANGUAGES := c:C cpp:CPP csharp:CSharp css:CSS go:Go html:HTML java:Java \
	javascript:JavaScript json:JSON kotlin:Kotlin lua:LUA php:PHP python:Python \
	r:R ruby:Ruby rust:Rust scala:Scala shellscript:ShellScript swift:Swift tsx:TSX \
	typescript:TypeScript vimscript:Vimscript xml:XML yaml:YAML

# Re-draws the whole RQ1 corpus, one CSV per language, with identical settings for every language.
# Previously only four languages had a target and the other fifteen CSVs were produced by ad-hoc
# invocations, some with --max-commits-per-repo 100 and some without - so the committed corpus
# mixed sampling methodologies. This target exists so the corpus is reproducible from one command.
#
# Deliberately ONE sampling run for every language, then split by the output's own `language`
# column - not one `--language` run per language. sample_code_pairs keys its reservoirs by
# (language, bucket) regardless of whether --language is passed; that flag only skips non-matching
# deltas *after* the commit walk. So N per-language runs re-walk all the repositories' full history
# N times over for no additional sampling power - measured at well over 10 minutes for a single
# language against the 100-repository corpus, i.e. hours for the set, against minutes for one pass.
#
# Stratification follows stats::sampling::LOC_BUCKETS (7 LOC buckets, keyed by the larger of
# before/after), so --count is split 7 ways per language. Re-run `make measure-rq1` afterwards to
# re-measure against the new sample.
# COUNT is per language, split evenly across the 7 LOC buckets, so the per-cell target is
# COUNT/7: COUNT=140 draws 20 pairs per language per bucket. Sparse cells (a language with few
# very large files, say) simply fill less; the reservoir cannot invent pairs that do not exist.
COUNT ?= 1000
sample-pairs-all: build
	$(BIN_DIR)/sample_code_pairs --path $(REPOSITORIES_DIR) --output data/samples/sampled_code_pairs_all.csv --count $(COUNT)
	@for entry in $(RQ1_LANGUAGES); do \
		suffix=$${entry%%:*}; language=$${entry##*:}; \
		out=data/samples/sampled_code_pairs_$$suffix.csv; \
		head -1 data/samples/sampled_code_pairs_all.csv > $$out; \
		awk -F, -v L="$$language" 'NR>1 && $$1==L' data/samples/sampled_code_pairs_all.csv >> $$out; \
		echo "$$language -> $$out ($$(( $$(wc -l < $$out) - 1 )) pairs)"; \
	done

# Rust-only variants, fixed at 1000 sampled pairs (sample_code_pairs' default --count and --seed,
# so re-running against the same checkouts reproduces the same pairs). This is the pipeline used to
# track diff_code's performance over time on real Rust commits; re-run measure-pairs-rust after
# any change to the diff algorithm to measure its effect.
sample-pairs-rust: build
	$(BIN_DIR)/sample_code_pairs --path $(REPOSITORIES_DIR) --output data/samples/sampled_code_pairs_rust.csv --language Rust

sample-pairs-java: build
	$(BIN_DIR)/sample_code_pairs --path $(REPOSITORIES_DIR) --output data/samples/sampled_code_pairs_java.csv --language Java --count 1000 --max-commits-per-repo 100

sample-pairs-javascript: build
	$(BIN_DIR)/sample_code_pairs --path $(REPOSITORIES_DIR) --output data/samples/sampled_code_pairs_javascript.csv --language JavaScript --count 1000 --max-commits-per-repo 100

sample-pairs-typescript: build
	$(BIN_DIR)/sample_code_pairs --path $(REPOSITORIES_DIR) --output data/samples/sampled_code_pairs_typescript.csv --language TypeScript --count 1000 --max-commits-per-repo 100

# ── Measurement ─────────────────────────────────────────────────────────────────────────────────

# Measure diff_code's speed, memory, AST size and mapping operation count across a sampled CSV.
measure-pairs: build
	$(BIN_DIR)/benchmark_diff_pairs --csv data/samples/sampled_code_pairs.csv --repo-root $(REPOSITORIES_DIR) --output data/diff_pairs_benchmark.csv

measure-pairs-rust: build
	$(BIN_DIR)/benchmark_diff_pairs --csv data/samples/sampled_code_pairs_rust.csv --repo-root $(REPOSITORIES_DIR) --output data/diff_pairs_benchmark_rust.csv

# Extended language targets with 20000 node limit
measure-pairs-java: build
	$(BIN_DIR)/benchmark_diff_pairs --csv data/samples/sampled_code_pairs_java.csv --repo-root $(REPOSITORIES_DIR) --output data/performance/benchmark_java.csv --max-combined-nodes 20000

measure-pairs-javascript: build
	$(BIN_DIR)/benchmark_diff_pairs --csv data/samples/sampled_code_pairs_javascript.csv --repo-root $(REPOSITORIES_DIR) --output data/performance/benchmark_javascript.csv --max-combined-nodes 20000

measure-pairs-typescript: build
	$(BIN_DIR)/benchmark_diff_pairs --csv data/samples/sampled_code_pairs_typescript.csv --repo-root $(REPOSITORIES_DIR) --output data/performance/benchmark_typescript.csv --max-combined-nodes 20000

# Size/LOC-changed statistics and distribution plots for sample-pairs-rust's output. Depends on
# sample-pairs-rust having already been run (needs the checked-out repositories, not just the CSV).
measure-code-pair-diffs:
	uv run ./analysis/code_pair_diff_stats.py data/samples/sampled_code_pairs_rust.csv --repo-root $(REPOSITORIES_DIR) --output-csv data/corpus_stats/code_pair_diff_stats_rust.csv

# Compares two measure-pairs-rust runs (e.g. before/after a diff_code algorithm change) and
# charts the difference. Usage: make measure-pairs-diff BEFORE=path/to/before.csv AFTER=path/to/after.csv
# (save a copy of data/diff_pairs_benchmark_rust.csv before re-running measure-pairs-rust, then
# pass that copy as BEFORE and the fresh run as AFTER).
measure-pairs-diff:
	@if [ -z "$(BEFORE)" ] || [ -z "$(AFTER)" ]; then \
		echo "usage: make measure-pairs-diff BEFORE=path/to/before.csv AFTER=path/to/after.csv" >&2; \
		exit 1; \
	fi
	uv run ./analysis/diff_pairs_benchmark_comparison.py --before $(abspath $(BEFORE)) --after $(abspath $(AFTER))

# Benchmark against existing sampled pairs for the four primary languages (Rust, Python, Go,
# Kotlin) and run analysis afterwards. Uses benchmark_all_extended.sh restricted to these four via
# --language.
measure-sampled-main-languages-only:
	@echo "Running benchmarks for Rust, Python, Go, Kotlin..."
	@echo "Results will be written to research/data/performance/"
	./measure/benchmark_all_extended.sh --language "Rust Python Go Kotlin" --repos-dir /var/tmp/research/small/repositories/ --bin-dir $(BIN_DIR)
	@echo ""
	@echo "Running analysis..."
	uv run ./analysis/benchmark_report.py

# Benchmark with extended language set (every language with a tree-sitter grammar - see
# ALL_LANGUAGES in benchmark_all_extended.sh) and higher node limit.
measure-sampled-all-languages:
	@echo "Running extended benchmarks for all supported languages..."
	@echo "Results will be written to research/data/performance/"
	@echo "Using 20000 node limit, max 100 commits per repo"
	./measure/benchmark_all_extended.sh \
		--language all \
		--repos-dir /var/tmp/research/small/repositories/ \
		--bin-dir $(BIN_DIR) \
		--limit 20000 \
		--max-commits 100 \
		--timeout-min 120 \
		--continue-on-error
	@echo ""
	@echo "Running analysis..."
	uv run ./analysis/benchmark_report.py

# Analysis/plotting step for the root Makefile's `benchmark-quality` (which is the product-side
# regression gate and stays there). Kept here because the report itself is a research artifact.
matching-reasons-report:
	$(MAKE) -C .. benchmark-quality
	uv run ./analysis/matching_reasons_report.py

# Change-shape census over the human-authored mappings: which shapes (reparent, sibling reorder,
# human-confirmed ambiguity) occur, how often, and how well codediff maps a fixture containing each.
# This is the "which shapes occur" half of the paper's RQ3, which the ablation study alone cannot
# answer. Reads data/quality/human_mapping_analysis.csv, so re-run `analyze_human_mappings --csv`
# first if fixtures were added - that file and data/quality/optimal_solutions_benchmark.csv must
# describe the same corpus state, since this report joins prevalence against codediff's own result.
shapes-report:
	uv run ./analysis/human_mapping_shapes_report.py

# Shape of real-world file edits: how many lines a commit touches, across how many files, and what
# fraction of a file an edit changes. Section 3 of the introductory paper cited only Arafat and
# Riehle's commit-size distribution for this and carried a TODO for our own numbers.
#
# Reads the cloned corpus under $(CORPUS_ROOT)/$(MODE)/repositories directly with `git log
# --numstat`, so it needs the clones (`make fetch MODE=<mode>`) but no build and no parse. It is a
# census of each clone's (depth-limited) history, NOT a draw from data/samples/ - those pairs are
# stratified by size bucket and so cannot answer "how big is a typical edit".
#
# EDIT_SHAPE_COMMITS caps how many commits per repository are walked, and defaults to the corpus's
# own clone depth. It is not a speed knob: these clones are shallow but NOT uniformly so, and an
# uncapped walk is dominated by torvalds-linux.git (1.29M of the corpus's 2.31M reachable commits),
# so it would report one repository's edit habits as the corpus's.
#
# Churn is derived from numstat plus the after-side blob's line count, not read from stats.sqlite:
# that database's lines_added/lines_removed/lines_changed/nodes_* columns are all zero, hardcoded
# by commit_stats.rs.
measure-edit-shape:
	uv run ./analysis/edit_shape_stats.py --repositories $(REPOSITORIES_DIR) --max-commits $(EDIT_SHAPE_COMMITS)

# Ground-truth ambiguity over the human-authored mappings: how often a real-world change admits no
# unique correct AST mapping, i.e. how often the annotator recorded a multi-map group. This is the
# paper's RQ3, and it mentions no diffing tool at all - unlike shapes-report above, nothing here
# joins against codediff's own result. Reads the mapping JSON files directly, scoped to the fixture
# set in data/quality/human_mapping_analysis.csv, so re-run `analyze_human_mappings --csv` first if
# fixtures were added. Also reads git history of those files (the multi-map facility postdates part
# of the corpus - see the script's own doc comment), so it needs a real checkout, not an export.
ambiguity-report:
	uv run ./analysis/ambiguity_report.py

# What reaches the reader, the paper's rendering questions. Two measurements in one report because
# the paper makes them one argument: the share of AST nodes that carry text of their own and can
# therefore be displayed at all (from data/quality/optimal_solutions_benchmark.csv's visible_nodes
# column, a property of the parsed files rather than of any diff of them), and how often the
# human-painted text ground truth records two equally correct renderings of the same change (from
# `text_mappings` in the mapping JSON files). Like ambiguity-report above, it names no diffing
# tool. Scoped to data/quality/human_mapping_analysis.csv, so re-run `analyze_human_mappings --csv`
# first if fixtures were added.
rendering-report:
	uv run ./analysis/rendering_report.py

# ── Tool comparison (benchmark_other) ───────────────────────────────────────────────────────────

# Runs the timing benchmark's own analysis/plotting step over whatever
# data/comparison/benchmark_other.csv already has on disk, without re-running the benchmark
# itself. Split out from measure-tools-timing below so a paper rebuild (introductory-paper) can
# re-render from existing data without paying for a fresh (slow - GumTree's JVM cold-starts per
# fixture) benchmark run.
# Codediff against the other diff tools (Unix `diff`, GumTree, difftastic, diffsitter) over this
# repository's own fixture corpus - timing, and agreement with the human ground truth.
#
# **In research/, not the root Makefile, because they measure other people's tools.** A benchmark
# answers "how good is codediff"; the moment a number depends on which version of GumTree happens
# to be installed in /var/tmp, it is a study of the field rather than product QA, and it belongs
# here with everything else whose answer moves when someone else ships a release. That is the rule
# even though these two do run against src/test/data/diffs/ and need no REPOSITORIES_DIR.
#
# `benchmark_other` itself stays in ../src/bin/: it is a dev tool gated behind `test-fixtures`, and
# nothing about it needed duplicating to move these targets - only the entry points moved.
#
# A tool that is not installed is skipped with a note, not an error, so both targets still produce
# a codediff column on a machine with none of them. See data/comparison/PROVENANCE.md for which
# builds are installed and which of their numbers are safe to quote.
#
# Timing is the only reason to pay for `measure-tools-timing`'s slow, machine-load-sensitive run.
# Its CSV does carry per-tool `*_mismatches` columns, but those are line-granularity only and are
# superseded by `measure-tools-accuracy`, which scores the same agreement at line, node, leaf and
# visible granularity. Both take `--fixtures a,b,c` to score a subset, and `--accuracy-csv` also
# takes `--tools name,...`; a scoped run parses only the fixtures asked for (0.6s against 18.8s for
# one fixture), which is worth knowing when iterating on a single tool adapter.
measure-tools-timing:
	cd .. && cargo run --release --features test-fixtures --bin benchmark_other -- --csv
	$(MAKE) timing-report

measure-tools-accuracy:
	cd .. && cargo run --release --features test-fixtures --bin benchmark_other -- --accuracy-csv

timing-report:
	uv run ./analysis/benchmark_other_report.py

# Installs BDiff (arXiv 2510.21094, block-aware text-based differencing) into a virtualenv under
# /var/tmp, matching where this project's other comparison tools live, and prints the BDIFF_PYTHON
# export the benchmark targets need.
#
# Two things this target exists to encode, both discovered the hard way (2026-08-23):
#
#  * BDiff's pyproject.toml under-declares its dependencies - it lists numpy and scipy, but
#    bdiff/bdiff.py also imports rapidfuzz. A plain `pip install .` produces a package that dies
#    on ModuleNotFoundError at first use, so rapidfuzz is installed explicitly here.
#  * BDiff shells out to `git diff --no-index`, so it inherits the user's git config. With
#    `diff.external` set - which this project's own README recommends - it silently returns an
#    empty edit script and scores as "nothing changed". benchmark_other neutralizes that per
#    invocation; see `git_env` there and data/comparison/PROVENANCE.md.
BDIFF_PREFIX ?= /var/tmp/bdiff-install
install-bdiff:
	mkdir -p $(BDIFF_PREFIX)
	test -d $(BDIFF_PREFIX)/BDiff || git clone --depth 1 https://github.com/BDiff/BDiff.git $(BDIFF_PREFIX)/BDiff
	test -d $(BDIFF_PREFIX)/venv || python3 -m venv $(BDIFF_PREFIX)/venv
	$(BDIFF_PREFIX)/venv/bin/pip install -q $(BDIFF_PREFIX)/BDiff rapidfuzz
	@$(BDIFF_PREFIX)/venv/bin/python -m bdiff --version
	@echo "export BDIFF_PYTHON=$(BDIFF_PREFIX)/venv/bin/python"

# ── RQ1: whole-tree APTED against a 1-second budget ─────────────────────────────────────────────

# Same slow/fast split as measure-tools-timing above: `rq1-report` re-renders from whatever
# data/rq1/apted_only_group*.csv already holds, `measure-rq1` re-measures first.
rq1-report:
	uv run ./analysis/apted_only_report.py

# SLOW (hours) and TIMING-SENSITIVE. Every pair gets a hard 1s budget enforced by killing a worker
# subprocess, so anything else competing for CPU turns pairs that would have finished into spurious
# timeouts. Run it on an otherwise-idle machine, and deliberately NOT in parallel across the four
# groups: this measures wall-clock against a fixed budget, so parallelism would corrupt exactly the
# quantity being measured. The four groups exist to keep each output file (and each restart, if one
# is interrupted) a manageable size, not to be run concurrently.
#
# MODE selects which checkout root the sampled pairs are resolved against; the sample CSVs
# themselves (data/samples/sampled_code_pairs_*.csv) are committed, so this needs no re-sampling step.
measure-rq1:
	cd .. && cargo build --release --features stats --bin apted_only_benchmark --bin apted_only_worker
	$(BIN_DIR)/apted_only_benchmark \
		--csv data/samples/sampled_code_pairs_css.csv data/samples/sampled_code_pairs_python.csv \
			data/samples/sampled_code_pairs_xml.csv data/samples/sampled_code_pairs_c.csv \
			data/samples/sampled_code_pairs_csharp.csv data/samples/sampled_code_pairs_vimscript.csv \
		--repo-root $(REPOSITORIES_DIR) --output data/rq1/apted_only_group1.csv
	$(BIN_DIR)/apted_only_benchmark \
		--csv data/samples/sampled_code_pairs_json.csv data/samples/sampled_code_pairs_typescript.csv \
			data/samples/sampled_code_pairs_go.csv data/samples/sampled_code_pairs_cpp.csv \
			data/samples/sampled_code_pairs_ruby.csv \
		--repo-root $(REPOSITORIES_DIR) --output data/rq1/apted_only_group2.csv
	$(BIN_DIR)/apted_only_benchmark \
		--csv data/samples/sampled_code_pairs_javascript.csv data/samples/sampled_code_pairs_yaml.csv \
			data/samples/sampled_code_pairs_shellscript.csv data/samples/sampled_code_pairs_swift.csv \
			data/samples/sampled_code_pairs_java.csv data/samples/sampled_code_pairs_kotlin.csv \
		--repo-root $(REPOSITORIES_DIR) --output data/rq1/apted_only_group3.csv
	$(BIN_DIR)/apted_only_benchmark \
		--csv data/samples/sampled_code_pairs_php.csv data/samples/sampled_code_pairs_tsx.csv \
			data/samples/sampled_code_pairs_rust.csv data/samples/sampled_code_pairs_html.csv \
			data/samples/sampled_code_pairs_lua.csv data/samples/sampled_code_pairs_r.csv \
			data/samples/sampled_code_pairs_scala.csv \
		--repo-root $(REPOSITORIES_DIR) --output data/rq1/apted_only_group4.csv
	$(MAKE) rq1-report

# ── Papers ──────────────────────────────────────────────────────────────────────────────────────

# Assembles every number in papers/introductory-paper into the single generated LaTeX macro file
# that paper's main.tex \inputs. See analysis/paper_variables.py's own module doc comment for which
# blocks are read back from a measurement artifact and which are transcribed with their provenance
# recorded alongside them.
#
# No copy step: papers/introductory-paper/figures/ symlinks into plots/, which is the single source
# of truth for every generated figure and for variables.tex.
#
# Invoked from the two paper targets below via a recipe-body $(MAKE), not listed as a prerequisite
# of either: it must run *after* whichever producer that target depends on (file-stats-report
# writes the empirical fragment this reads), and prerequisite order isn't guaranteed under
# `make -j`. Same recipe-body-$(MAKE) convention measure-tools-timing uses for its own report step.
paper-variables:
	uv run ./analysis/paper_variables.py

# Rebuilds the paper PDF from the figures and macros already in plots/.
#
# Deliberately does NOT depend on timing-report: that step regenerates the comparison
# charts and variance table from whatever data/comparison/benchmark_other.csv currently holds,
# and that CSV (156 fixtures) has grown past the corpus every AUTHORED number in the paper was
# measured on (98 fixtures - see paper_variables.py's COMPARISON block and the TODO in main.tex).
# A routine PDF rebuild must not silently swap 156-fixture charts into a paper whose prose says
# 98 - exactly that happened live on 2026-08-19, caught only because the regenerated variance
# table's n column changed. When the evaluation is deliberately refreshed, run
# `make measure-tools-timing` (or timing-report), update the authored blocks in
# paper_variables.py to match, and re-render - together, never piecemeal.
#
# Also does not regenerate plots/tips.png, which needs the full repository-fetch pipeline
# (`make full`). Needs a LaTeX toolchain with the acmart class and cm-super (see
# papers/introductory-paper/README.md). `-g` forces a full latexmk run regardless of its own
# up-to-date check: without it, latexmk can decide main.pdf is already current from main.tex's own
# timestamp alone and skip the rebuild even though an \input-ed generated table just changed
# underneath it (confirmed live, 2026-07-31 - main.pdf was silently one run stale until this flag
# was added).
#
# `build-pdf` falls back to a manual pdflatex+bibtex sequence when latexmk is not installed
# (discovered live, 2026-08-19: latexmk absent on the primary dev machine while pdflatex worked
# fine, silently breaking both paper targets). Three pdflatex passes is the standard fixed point
# for cross-references + bibliography; byproducts are removed afterward, matching what
# `latexmk -c` would leave.
define build-pdf
	cd papers/introductory-paper && \
	if command -v latexmk >/dev/null 2>&1; then \
		latexmk -pdf -g -interaction=nonstopmode main.tex; \
	else \
		pdflatex -interaction=nonstopmode main.tex && bibtex main && \
		pdflatex -interaction=nonstopmode main.tex && \
		pdflatex -interaction=nonstopmode main.tex && \
		rm -f main.aux main.log main.out main.blg main.fls main.fdb_latexmk; \
	fi
endef

introductory-paper:
	$(MAKE) paper-variables
	$(build-pdf)

# Regenerates papers/introductory-paper's empirical-study numbers (Table 1, repository/file/language
# counts, bytes-AST correlation - all \input from figures/variables.tex, a generated LaTeX-macro
# file, not hand-transcribed - see write_paper_variables's own doc comment in analysis/file_stats.py
# for exactly why that matters) and its file-types figure, from whatever $(RESEARCH_DIR)/stats.sqlite
# already has on disk for the current MODE (default tiny - pass MODE=small or MODE=full to match
# whatever `make measure-file-stats` run you actually have). Reuses the fast half of that split
# (file-stats-report), not measure-file-stats itself, for the same reason introductory-paper depends on
# timing-report and not measure-tools-timing: rebuilding a paper should never pay for the slow
# step. Run `make measure-file-stats MODE=<mode>` yourself first to (re)populate that mode's stats.sqlite.
introductory-paper-empirical: file-stats-report
	$(MAKE) paper-variables
	$(build-pdf)
