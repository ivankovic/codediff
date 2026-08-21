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

# Just the analysis/plotting/paper-variables step of file-stats, over whatever
# $(RESEARCH_DIR)/stats.sqlite already has on disk, without re-running file_stats itself. Split
# out for the same reason as benchmark-timing-report below - see introductory-paper-empirical,
# which depends on this, not on file-stats.
file-stats-report:
	uv run ./analysis/file_stats.py $(RESEARCH_DIR)/stats.sqlite

file-stats: build
	$(BIN_DIR)/file_stats --path $(REPOSITORIES_DIR) --db $(RESEARCH_DIR)/stats.sqlite
	$(MAKE) file-stats-report

commit-stats: build
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
analyze: file-stats

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
# before/after), so --count is split 7 ways per language. Re-run `make rq1` afterwards to
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
# track diff_code's performance over time on real Rust commits; re-run benchmark-pairs-rust after
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
benchmark-pairs: build
	$(BIN_DIR)/benchmark_diff_pairs --csv data/samples/sampled_code_pairs.csv --repo-root $(REPOSITORIES_DIR) --output data/diff_pairs_benchmark.csv

benchmark-pairs-rust: build
	$(BIN_DIR)/benchmark_diff_pairs --csv data/samples/sampled_code_pairs_rust.csv --repo-root $(REPOSITORIES_DIR) --output data/diff_pairs_benchmark_rust.csv

# Extended language targets with 20000 node limit
benchmark-pairs-java: build
	$(BIN_DIR)/benchmark_diff_pairs --csv data/samples/sampled_code_pairs_java.csv --repo-root $(REPOSITORIES_DIR) --output data/performance/benchmark_java.csv --max-combined-nodes 20000

benchmark-pairs-javascript: build
	$(BIN_DIR)/benchmark_diff_pairs --csv data/samples/sampled_code_pairs_javascript.csv --repo-root $(REPOSITORIES_DIR) --output data/performance/benchmark_javascript.csv --max-combined-nodes 20000

benchmark-pairs-typescript: build
	$(BIN_DIR)/benchmark_diff_pairs --csv data/samples/sampled_code_pairs_typescript.csv --repo-root $(REPOSITORIES_DIR) --output data/performance/benchmark_typescript.csv --max-combined-nodes 20000

# Size/LOC-changed statistics and distribution plots for sample-pairs-rust's output. Depends on
# sample-pairs-rust having already been run (needs the checked-out repositories, not just the CSV).
code-pair-diff-stats:
	uv run ./analysis/code_pair_diff_stats.py data/samples/sampled_code_pairs_rust.csv --repo-root $(REPOSITORIES_DIR) --output-csv data/corpus_stats/code_pair_diff_stats_rust.csv

# Compares two benchmark-pairs-rust runs (e.g. before/after a diff_code algorithm change) and
# charts the difference. Usage: make benchmark-pairs-diff BEFORE=path/to/before.csv AFTER=path/to/after.csv
# (save a copy of data/diff_pairs_benchmark_rust.csv before re-running benchmark-pairs-rust, then
# pass that copy as BEFORE and the fresh run as AFTER).
benchmark-pairs-diff:
	@if [ -z "$(BEFORE)" ] || [ -z "$(AFTER)" ]; then \
		echo "usage: make benchmark-pairs-diff BEFORE=path/to/before.csv AFTER=path/to/after.csv" >&2; \
		exit 1; \
	fi
	uv run ./analysis/diff_pairs_benchmark_comparison.py --before $(abspath $(BEFORE)) --after $(abspath $(AFTER))

# Benchmark against existing sampled pairs for the four primary languages (Rust, Python, Go,
# Kotlin) and run analysis afterwards. Uses benchmark_all_extended.sh restricted to these four via
# --language.
benchmark-sampled-main-languages-only:
	@echo "Running benchmarks for Rust, Python, Go, Kotlin..."
	@echo "Results will be written to research/data/performance/"
	./measure/benchmark_all_extended.sh --language "Rust Python Go Kotlin" --repos-dir /var/tmp/research/small/repositories/ --bin-dir $(BIN_DIR)
	@echo ""
	@echo "Running analysis..."
	uv run ./analysis/benchmark_report.py

# Benchmark with extended language set (every language with a tree-sitter grammar - see
# ALL_LANGUAGES in benchmark_all_extended.sh) and higher node limit.
benchmark-sampled-all-languages:
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

# Leave-one-out ablation study over the diff algorithm's optional heuristic passes - see
# measure/ablation_study.sh's own header comment for exactly what it measures.
# Usage: make ablation-study [OUT_DIR=path]  (default OUT_DIR: research/data/ablation)
ablation-study:
	./measure/ablation_study.sh $(OUT_DIR)

# Analysis/plotting step for the root Makefile's `benchmark-optimal` (which is the product-side
# regression gate and stays there). Kept here because the report itself is a research artifact.
benchmark-optimal-report:
	$(MAKE) -C .. benchmark-optimal
	uv run ./analysis/matching_reasons_report.py

# ── Tool comparison (benchmark_other) ───────────────────────────────────────────────────────────

# Runs the timing benchmark's own analysis/plotting step over whatever
# data/comparison/benchmark_other.csv already has on disk, without re-running the benchmark
# itself. Split out from benchmark-timing below so a paper rebuild (introductory-paper) can
# re-render from existing data without paying for a fresh (slow - GumTree's JVM cold-starts per
# fixture) benchmark run.
benchmark-timing-report:
	uv run ./analysis/benchmark_other_report.py

# Compares codediff against other diff tools (Unix diff, GumTree, difftastic, diffsitter) on
# runtime, plus line-level agreement with the human-authored mapping, then runs the analysis step
# above. --features test-fixtures: benchmark_other, like benchmark_optimal_solutions, needs
# codediff::test's fixture-loading helpers.
#
# Requires GUMTREE_BIN, DIFFT_BIN and DIFFSITTER_BIN, each pointing at a built binary - unlike
# everything else here, these are external, non-Rust (GumTree) or at least non-workspace
# (difftastic, diffsitter) tool dependencies this target can't provide on its own. All three are
# required for a full run: a missing one fails loudly on the first fixture in that tool's language
# scope rather than silently skipping the tool. difftastic and diffsitter are plain
# `cargo install`-able; install them into a scratch prefix to keep them out of the system-wide
# cargo bin directory:
#
#     cargo install --root /var/tmp/codediff-tools difftastic diffsitter
#     export DIFFT_BIN=/var/tmp/codediff-tools/bin/difft \
#            DIFFSITTER_BIN=/var/tmp/codediff-tools/bin/diffsitter
#
# GumTree is a built Java distribution, so GUMTREE_BIN points at its `bin/gumtree` script. See
# drivers/gumtree-batch/build.sh for its optional warm-JVM timing variant, and
# data/comparison/PROVENANCE.md for which GumTree build is installed and which of its numbers are
# safe to quote.
#
# Named for timing because that is what only this target measures. Its CSV does also carry
# per-tool `*_mismatches` columns, but those are line-granularity only and are superseded by
# benchmark-accuracy below, which scores the same agreement at line, node, leaf and visible
# granularity. Timing is the reason to pay for this target's slow, machine-load-sensitive run.
# (Renamed from `benchmark-other` 2026-08-20; the binary, CSV and report script keep their
# `benchmark_other` names, since the binary genuinely serves both this target and
# benchmark-accuracy.)
benchmark-timing:
	cd .. && cargo run --release --features test-fixtures --bin benchmark_other -- --csv
	$(MAKE) benchmark-timing-report

# Accuracy-only counterpart of benchmark-timing: per fixture, every tool's agreement with the
# human mapping at line, node, leaf-node and visible-node granularity, written to
# data/comparison/benchmark_accuracy.csv with sample.csv provenance columns for cross-referencing.
# No timing at all, so unlike benchmark-timing this doesn't need the warm-JVM driver and doesn't
# care about machine load - accuracy is deterministic. Same GUMTREE_BIN / DIFFT_BIN /
# DIFFSITTER_BIN requirement.
#
# The node columns are a "did the tool consider this node's text changed" projection, NOT the
# node-mapping fidelity benchmark_optimal_solutions reports - see data/comparison/PROVENANCE.md
# and `benchmark_other --help`. Unix diff has no node columns by construction.
#
# The `*_visible_node_mismatches` columns restrict that projection to nodes that actually reach
# the screen when the diff is rendered. Visibility is judged against the *human* mapping, so every
# tool is scored against one fixed, tool-independent set of visible nodes - deliberately unlike
# benchmark_optimal_solutions' own visible count, which judges codediff by codediff's own
# rendering. Visible is not a synonym for leaf; see PROVENANCE.md for why they differ both ways.
benchmark-accuracy:
	cd .. && cargo run --release --features test-fixtures --bin benchmark_other -- --accuracy-csv

# ── RQ1: whole-tree APTED against a 1-second budget ─────────────────────────────────────────────

# Same slow/fast split as benchmark-timing above: `rq1-report` re-renders from whatever
# data/rq1/apted_only_group*.csv already holds, `rq1` re-measures first.
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
rq1:
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
# `make -j`. Same recipe-body-$(MAKE) convention benchmark-timing uses for its own report step.
paper-variables:
	uv run ./analysis/paper_variables.py

# Rebuilds the paper PDF from the figures and macros already in plots/.
#
# Deliberately does NOT depend on benchmark-timing-report: that step regenerates the comparison
# charts and variance table from whatever data/comparison/benchmark_other.csv currently holds,
# and that CSV (156 fixtures) has grown past the corpus every AUTHORED number in the paper was
# measured on (98 fixtures - see paper_variables.py's COMPARISON block and the TODO in main.tex).
# A routine PDF rebuild must not silently swap 156-fixture charts into a paper whose prose says
# 98 - exactly that happened live on 2026-08-19, caught only because the regenerated variance
# table's n column changed. When the evaluation is deliberately refreshed, run
# `make benchmark-timing` (or benchmark-timing-report), update the authored blocks in
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
# whatever `make file-stats` run you actually have). Reuses the fast half of that split
# (file-stats-report), not file-stats itself, for the same reason introductory-paper depends on
# benchmark-timing-report and not benchmark-timing: rebuilding a paper should never pay for the slow
# step. Run `make file-stats MODE=<mode>` yourself first to (re)populate that mode's stats.sqlite.
introductory-paper-empirical: file-stats-report
	$(MAKE) paper-variables
	$(build-pdf)
