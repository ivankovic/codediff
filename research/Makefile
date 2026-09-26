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

.PHONY: build require-full-corpus clean clean-db fetch file-stats-report measure-file-stats reclassify-tips measure-commit-stats \
	debug-stats analyze tiny small full sample-pairs sample-pairs-all measure-pairs \
	measure-code-pair-diffs pairs-diff-report measure-sampled matching-reasons-report \
	shapes-report measure-edit-shape ambiguity-report rendering-report measure-tools-timing \
	measure-tools-accuracy timing-report install-bdiff install-srcdiff apted-budget-report measure-apted-budget \
	paper-variables introductory-paper introductory-paper-empirical fetch-external \
	measure-astdiff-oracle measure-astdiff-oracle-human measure-robustness-fixtures

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

# Recomputes the `tip` (file type) column of an existing stats.sqlite from the stored paths with
# the current src/code/tip.rs tables, without touching the checkouts. Dry-run: prints the category
# shares before and after plus what is still unclassified. Add RECLASSIFY_FLAGS=--write to apply,
# then `make file-stats-report` to regenerate the file-type figure, and record the reclassification
# in that corpus's PROVENANCE.md. See src/bin/reclassify_tips.rs for what such a row does and does
# not carry afterwards.
reclassify-tips: build
	$(BIN_DIR)/reclassify_tips --db $(RESEARCH_DIR)/stats.sqlite $(RECLASSIFY_FLAGS)

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
# A language missing here while `sample_code_pairs` samples it gets no per-language CSV and is
# never measured - a silent gap between what the corpus covers and what RQ1 reports on, not an
# error anything surfaces. Keep this list in sync with the languages the sampler actually
# produces (check `sampled_code_pairs_all.csv`'s own `language` column, not this list).
LANGUAGES := c:C cpp:CPP csharp:CSharp css:CSS go:Go html:HTML java:Java \
	javascript:JavaScript json:JSON kotlin:Kotlin lua:LUA php:PHP python:Python \
	r:R ruby:Ruby rust:Rust scala:Scala shellscript:ShellScript swift:Swift tsx:TSX \
	typescript:TypeScript vimscript:Vimscript xml:XML yaml:YAML

# Re-draws the whole sampled corpus, one CSV per language, with identical settings for every
# language.
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
# before/after), so --count is split 7 ways per language. Re-run `make measure-apted-budget` afterwards to
# re-measure against the new sample.
# COUNT is per language, split evenly across the 7 LOC buckets, so the per-cell target is
# COUNT/7: COUNT=140 draws 20 pairs per language per bucket. Sparse cells (a language with few
# very large files, say) simply fill less; the reservoir cannot invent pairs that do not exist.
COUNT ?= 1000
sample-pairs-all: build
	$(BIN_DIR)/sample_code_pairs --path $(REPOSITORIES_DIR) --output data/samples/sampled_code_pairs_all.csv --count $(COUNT)
	@for entry in $(LANGUAGES); do \
		suffix=$${entry%%:*}; language=$${entry##*:}; \
		out=data/samples/sampled_code_pairs_$$suffix.csv; \
		head -1 data/samples/sampled_code_pairs_all.csv > $$out; \
		awk -F, -v L="$$language" 'NR>1 && $$1==L' data/samples/sampled_code_pairs_all.csv >> $$out; \
		echo "$$language -> $$out ($$(( $$(wc -l < $$out) - 1 )) pairs)"; \
	done

# There are deliberately no per-language sampling targets. `sample-pairs-all`, `-java`,
# `-javascript` and `-typescript` used to exist and were deleted: they wrote to the *same*
# data/samples/sampled_code_pairs_<lang>.csv files `sample-pairs-all` produces, with different
# settings (rust passed neither --count nor --max-commits-per-repo; the other three passed both),
# so running one silently replaced part of a consistently-drawn corpus with a differently-drawn
# one. That is the exact failure the comment above sample-pairs-all describes, still live in four
# targets next to the target written to fix it - and they cost hours to do it, since each re-walks
# every repository's full history for one language's worth of pairs.
#
# Draw one language by drawing them all: `make sample-pairs-all` is one pass and splits by the
# output's own `language` column.

# ── Measurement ─────────────────────────────────────────────────────────────────────────────────

# Measure diff_code's speed, memory, AST size and mapping operation count across one language's
# sampled pairs.
#
# One LANGUAGE-parameterised target, not the five it replaced (`measure-pairs`, `-rust`, `-java`,
# `-javascript`, `-typescript`). Those ran the same binary with per-target opinions nobody had
# written down: three passed --max-combined-nodes 20000 and two did not, and they wrote to two
# different conventions - data/diff_pairs_benchmark[_rust].csv loose in data/, against
# data/performance/benchmark_<language>.csv for the rest. The second is canonical: it is what
# measure/benchmark_all_extended.sh writes, what analysis/benchmark_report.py reads, and what the
# 20-odd committed per-language CSVs in data/performance/ already use. The two loose paths were
# never committed, so unifying on the canonical one moves no data.
#
#   make measure-pairs LANGUAGE=rust
#   make measure-pairs LANGUAGE=all PAIRS_NODE_LIMIT=0     # 0 disables the cap
#
# Input is data/samples/sampled_code_pairs_$(LANGUAGE).csv, which `sample-pairs-all` produces for
# every language in $(LANGUAGES) in one pass.
PAIRS_NODE_LIMIT ?= 20000

# The committed samples (data/samples/) were drawn from the full corpus, so any other MODE resolves
# none of their pairs, and the measure targets that read them would overwrite committed results
# with `failed_to_read` rows. Their first prerequisite, so it fails before the release build.
require-full-corpus:
	@if [ "$(MODE)" != full ] || [ ! -d "$(REPOSITORIES_DIR)" ]; then \
		echo "$(MAKECMDGOALS) measures the committed samples, which come from the full corpus:" >&2; \
		echo "run 'make fetch MODE=full' once, then 'make $(MAKECMDGOALS) MODE=full'" >&2; \
		exit 1; \
	fi

measure-pairs: require-full-corpus build
	@if [ -z "$(LANGUAGE)" ]; then \
		echo "usage: make measure-pairs LANGUAGE=<slug>" >&2; \
		echo "slugs: $(foreach e,$(LANGUAGES),$(firstword $(subst :, ,$(e)))) all" >&2; \
		exit 1; \
	fi
	@if [ ! -f data/samples/sampled_code_pairs_$(LANGUAGE).csv ]; then \
		echo "no data/samples/sampled_code_pairs_$(LANGUAGE).csv - run 'make sample-pairs-all' first" >&2; \
		exit 1; \
	fi
	$(BIN_DIR)/benchmark_diff_pairs \
		--csv data/samples/sampled_code_pairs_$(LANGUAGE).csv \
		--repo-root $(REPOSITORIES_DIR) \
		--output data/performance/benchmark_$(LANGUAGE).csv \
		--max-combined-nodes $(PAIRS_NODE_LIMIT)

# The paper's robustness run: every `small`/`full`/`stratified`/`defects4j` fixture directory (the datasets the
# paper reports on) pushed through diff_code with no node cap, a per-fixture timeout, panic
# isolation and the allocation counters, into one CSV with the same schema as measure-pairs.
# Over the fixture corpus rather than a sample of clones, so it covers the corpus the rest of
# the paper is measured on, exercises the large files the Robust target is about, and needs no
# checkouts and no MODE - a sample's clones rewrite pairs out of history between runs. A few
# minutes; feeds
# analysis/paper_variables.py's ROBUSTNESS block.
measure-robustness-fixtures: build
	$(BIN_DIR)/benchmark_diff_pairs --fixtures \
		--output data/performance/robustness_fixtures.csv \
		--max-combined-nodes 1000000000 --timeout-secs 120 --iterations 5

# The paper's robustness run over the WHOLE Full corpus (every modified code file in its recent
# history, no node cap), not the fixture corpus above. Hours of eight-core time and a memory-capped
# systemd unit rather than a make recipe, because an unconfined run can take the machine down;
# this target only starts it. See measure/overnight_benchmarks.sh's header for the
# knobs, measure/r48_retry_killed.sh for the pass that follows, and data/performance/PROVENANCE.md.
measure-robustness-full:
	systemd-run --user --unit codediff-overnight --collect -p WorkingDirectory=$(CURDIR) \
		--setenv=PATH="$$PATH" --setenv=HOME="$$HOME" --setenv=SKIP_R22=1 \
		./measure/overnight_benchmarks.sh
	@echo "started as codediff-overnight; follow with: journalctl --user -u codediff-overnight -f"

measure-code-pair-diffs:
	uv run ./analysis/code_pair_diff_stats.py data/samples/sampled_code_pairs_$(LANGUAGE).csv \
		--repo-root $(REPOSITORIES_DIR) \
		--output-csv data/corpus_stats/code_pair_diff_stats_$(LANGUAGE).csv

# Compares two `measure-pairs` runs (e.g. before/after a diff_code algorithm change) and
# charts the difference. Usage: make pairs-diff-report BEFORE=path/to/before.csv AFTER=path/to/after.csv
# (save a copy of data/performance/benchmark_<language>.csv before re-running measure-pairs, then
# pass that copy as BEFORE and the fresh run as AFTER).
pairs-diff-report:
	@if [ -z "$(BEFORE)" ] || [ -z "$(AFTER)" ]; then \
		echo "usage: make pairs-diff-report BEFORE=path/to/before.csv AFTER=path/to/after.csv" >&2; \
		exit 1; \
	fi
	uv run ./analysis/diff_pairs_benchmark_comparison.py --before $(abspath $(BEFORE)) --after $(abspath $(AFTER))

# Speed and memory across a whole sampled corpus, via measure/benchmark_all_extended.sh.
#
# One target, LANGUAGE-parameterised, rather than the two it replaced
# (`measure-sampled` and `measure-sampled`). Those differed only
# in the flags they passed - the narrow one silently dropped --limit, --max-commits, --timeout-min
# and --continue-on-error, so "the same benchmark on fewer languages" was not the same benchmark.
# Naming a language set is not a reason to have a second copy of the settings.
#
#   make measure-sampled                                  # every supported language
#   make measure-sampled LANGUAGE="Rust Python Go Kotlin"  # a subset
# Deliberately no default. `measure-sampled` treats an unset LANGUAGE as "every language"; for
# `measure-pairs` there is no sensible default and an unset one has to be an error, not a guess -
# a shared `LANGUAGE ?= all` made the second silently measure the whole corpus.
SAMPLED_REPOS ?= /var/tmp/research/small/repositories/
SAMPLED_NODE_LIMIT ?= 20000
SAMPLED_MAX_COMMITS ?= 100
SAMPLED_TIMEOUT_MIN ?= 120
measure-sampled:
	@echo "Benchmarking $(or $(LANGUAGE),all); results go to research/data/performance/"
	./measure/benchmark_all_extended.sh \
		--language "$(or $(LANGUAGE),all)" \
		--repos-dir $(SAMPLED_REPOS) \
		--bin-dir $(BIN_DIR) \
		--limit $(SAMPLED_NODE_LIMIT) \
		--max-commits $(SAMPLED_MAX_COMMITS) \
		--timeout-min $(SAMPLED_TIMEOUT_MIN) \
		--continue-on-error
	uv run ./analysis/benchmark_report.py

# Analysis/plotting step for the root Makefile's `benchmark-quality` (which is the product-side
# regression gate and stays there). Kept here because the report itself is a research artifact.
# Re-renders from data/quality/optimal_solutions_benchmark.csv on disk, like every other
# `*-report` target; the root Makefile's `benchmark-quality` is what produces that file.
matching-reasons-report:
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
# fixtures were added. One rate over every in-scope fixture, see the script's own doc comment.
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

# Line coverage of every test in the library's test binary, each measured alone, so the results
# can be combined with set operations (analysis/coverage_sets.py). Builds an instrumented release
# test binary in a target directory of its own - so neither `make coverage`'s nor the release
# build's artifacts are touched - then starts measure/per_test_coverage.py as a systemd unit, each
# test in its own memory-capped scope; see that script for why. About ten minutes on eight cores.
#
#   journalctl --user -fu codediff-per-test-coverage     follow it
#   systemctl --user stop codediff-per-test-coverage codediff-per-test-coverage.slice
#
# Resumable: run it again to continue. To re-measure from scratch - after the engine changes, say -
# delete data/coverage/per_test/ first, since the bits only mean anything against the binary that
# wrote them. PROVENANCE.md there records which commit that was.
PER_TEST_COVERAGE_TARGET := $(abspath ../target/percov)
measure-per-test-coverage:
	cd .. && eval "$$(cargo llvm-cov show-env --export-prefix)" \
		&& CARGO_TARGET_DIR=$(PER_TEST_COVERAGE_TARGET) cargo test --release --lib --no-run \
		--message-format=json 2>/dev/null \
		| python3 -c 'import sys, json; print(next(m["executable"] for m in map(json.loads, sys.stdin) if m.get("reason") == "compiler-artifact" and m.get("profile", {}).get("test") and m.get("executable")))' \
		> $(PER_TEST_COVERAGE_TARGET)/test-binary.txt
	mkdir -p data/coverage/per_test
	systemd-run --user --unit codediff-per-test-coverage --collect --same-dir \
		--setenv=PATH="$$PATH" ./measure/per_test_coverage.py \
		--binary "$$(cat $(PER_TEST_COVERAGE_TARGET)/test-binary.txt)" --out data/coverage/per_test

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

# The paper's two distribution figures (corpus shape; the one-second budget), every size-or-time
# distribution drawn as a cumulative curve in one idiom. Reads only committed artifacts - the
# value->count files file-stats-report and measure-edit-shape write, data/rq1/*.csv and
# data/comparison/benchmark_other.csv - so it is always cheap to re-run and never re-measures.
distributions-report:
	uv run ./analysis/distributions_report.py

# Installs BDiff (arXiv 2510.21094, block-aware text-based differencing) into a virtualenv under
# /var/tmp, matching where this project's other comparison tools live, and prints the BDIFF_PYTHON
# export the benchmark targets need.
#
# Two things this target exists to encode:
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

# Builds srcDiff (Decker et al., TOSEM 2020; syntactic differencing over srcML) into /var/tmp,
# and prints the SRCDIFF_BIN export the benchmark targets need. Needs no root.
#
#  * srcDiff's development tip calls srcml_unit_get_archive, which no srcML release has (1.1.0 is
#    the latest, 2025-08), so srcML is built from its development branch too. Both are pinned: a
#    moving tip is a different tool next month.
#  * srcML's build needs the -dev headers of libxml2, libxslt, libarchive, libcurl and ICU, whose
#    runtime libraries Ubuntu already has. `apt download` + `dpkg -x` unpacks them into a sysroot,
#    and the dangling .so symlinks are repointed at the system libraries. Also needs Java (srcML
#    generates its parser with ANTLR 2) and cmake >= 3.28, installed here from PyPI with ninja.
SRCDIFF_PREFIX ?= /var/tmp/srcdiff-install
SRCDIFF_COMMIT ?= ef42b332b2e9400e441a8049c4ee170a2a2203e8
SRCML_COMMIT ?= c9f001484253b8b837b1c3db81fa553bc69301ce
install-srcdiff:
	mkdir -p $(SRCDIFF_PREFIX)/debs
	test -d $(SRCDIFF_PREFIX)/venv || python3 -m venv $(SRCDIFF_PREFIX)/venv
	$(SRCDIFF_PREFIX)/venv/bin/pip install -q cmake ninja
	cd $(SRCDIFF_PREFIX)/debs && apt download libxml2-dev libxslt1-dev libarchive-dev \
		libcurl4-openssl-dev libicu-dev && for d in *.deb; do dpkg -x $$d ../sysroot; done
	cd $(SRCDIFF_PREFIX)/sysroot/usr/lib/x86_64-linux-gnu && for f in *.so; do \
		t=$$(readlink $$f); test -e "$$t" || ln -sfn /usr/lib/x86_64-linux-gnu/$$t $$f; done
	test -d $(SRCDIFF_PREFIX)/srcML || git clone https://github.com/srcML/srcML.git $(SRCDIFF_PREFIX)/srcML
	test -d $(SRCDIFF_PREFIX)/srcDiff || git clone https://github.com/srcDiff/srcDiff.git $(SRCDIFF_PREFIX)/srcDiff
	git -C $(SRCDIFF_PREFIX)/srcML checkout -q $(SRCML_COMMIT)
	git -C $(SRCDIFF_PREFIX)/srcDiff checkout -q $(SRCDIFF_COMMIT)
	git -C $(SRCDIFF_PREFIX)/srcDiff submodule update --init --recursive
	I=$(SRCDIFF_PREFIX)/sysroot/usr/include; \
	export PATH=$(SRCDIFF_PREFIX)/venv/bin:$$PATH CPLUS_INCLUDE_PATH=$$I:$$I/x86_64-linux-gnu \
		C_INCLUDE_PATH=$$I:$$I/x86_64-linux-gnu LIBRARY_PATH=$(SRCDIFF_PREFIX)/sysroot/usr/lib/x86_64-linux-gnu; \
	cmake -S $(SRCDIFF_PREFIX)/srcML -B $(SRCDIFF_PREFIX)/srcml-build -G Ninja -DCMAKE_BUILD_TYPE=Release \
		-DCMAKE_PREFIX_PATH=$(SRCDIFF_PREFIX)/sysroot/usr -DCMAKE_INSTALL_PREFIX=$(SRCDIFF_PREFIX)/srcml-head \
		-DBUILD_CLIENT_TESTS=OFF && \
	cmake --build $(SRCDIFF_PREFIX)/srcml-build && cmake --install $(SRCDIFF_PREFIX)/srcml-build && \
	cd $(SRCDIFF_PREFIX)/srcDiff && \
	cmake --preset debian -DCMAKE_PREFIX_PATH="$(SRCDIFF_PREFIX)/srcml-head;$(SRCDIFF_PREFIX)/sysroot/usr" && \
	cmake --build build
	@$(SRCDIFF_PREFIX)/srcDiff/build/bin/srcdiff -V
	@echo "export SRCDIFF_BIN=$(SRCDIFF_PREFIX)/srcDiff/build/bin/srcdiff"

# ── External oracles: ground truth we did not write ─────────────────────────────────────────────

# Third-party datasets, fetched into /var/tmp/research/external/ by the scripts in external/ (each
# is idempotent and records what it fetched in a FETCHED file). See external/README.md for what
# they are and why. Nothing here is vendored into the repository.
fetch-external:
	./external/fetch_astdiff_oracle.sh
	./external/fetch_gumtree_simple_package.sh
	./external/fetch_defects4j_metadata.sh

# Score codediff's node mapping against the Alikhanifard & Tsantalis oracle (Defects4J half, 800
# cases). Writes data/comparison/astdiff_oracle_defects4j.csv and prints precision/recall at the
# paper's two granularities next to its published numbers. Reads ~2.2 GB of oracle JSON twice;
# takes a few minutes. `--details` adds the unresolved-JDT-type census, the kind whitelist and
# every FP/FN with source text; `--cases Time-20,Chart-1` scopes a run.
measure-astdiff-oracle: fetch-external
	cd .. && cargo run --release --features test-fixtures --bin benchmark_astdiff_oracle -- \
		--csv research/data/comparison/astdiff_oracle_defects4j.csv $(ASTDIFF_ORACLE_ARGS)

# The same oracle, with our own hand-authored mappings scored against it instead of codediff's
# output, over the Defects4J compilation units solved so far. Writes
# data/comparison/astdiff_oracle_defects4j_human.csv; the paper reads both CSVs together, and
# codediff's own rows over the same units are the control that makes the figure readable.
measure-astdiff-oracle-human: fetch-external
	cd .. && cargo run --release --features test-fixtures --bin benchmark_astdiff_oracle -- \
		--human-mappings src/test/data/diffs/defects4j \
		--csv research/data/comparison/astdiff_oracle_defects4j_human.csv $(ASTDIFF_ORACLE_ARGS)

# ── RQ1: whole-tree APTED against a 1-second budget ─────────────────────────────────────────────

# Same slow/fast split as measure-tools-timing above: `apted-budget-report` re-renders from whatever
# data/rq1/apted_only_group*.csv already holds, `measure-apted-budget` re-measures first.
apted-budget-report:
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
# The four groups, as language keys (the sample CSV is data/samples/sampled_code_pairs_<key>.csv).
# One variable per group so a re-grouping edits a list, not four copied recipes.
RQ1_GROUP_1 := css python xml c csharp vimscript
RQ1_GROUP_2 := json typescript go cpp ruby
RQ1_GROUP_3 := javascript yaml shellscript swift java kotlin
RQ1_GROUP_4 := php tsx rust html lua r scala
rq1-sample-csvs = $(foreach lang,$(1),data/samples/sampled_code_pairs_$(lang).csv)
measure-apted-budget: require-full-corpus build
	$(foreach g,1 2 3 4,$(BIN_DIR)/apted_only_benchmark \
		--csv $(call rq1-sample-csvs,$(RQ1_GROUP_$(g))) \
		--repo-root $(REPOSITORIES_DIR) --output data/rq1/apted_only_group$(g).csv &&) true
	$(MAKE) apted-budget-report

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
# Deliberately does NOT depend on timing-report: that step regenerates the comparison charts,
# the variance table and `plots/variables_comparison.tex` from whatever
# data/comparison/benchmark_other.csv and benchmark_accuracy.csv currently hold. A routine PDF
# rebuild must not silently swap in charts measured on a different corpus than the prose was
# written against; the only thing that would catch it is a changed n column in the regenerated
# variance table.
#
# The per-tool numbers are generated, so
# a refresh is `make measure-tools-accuracy` and then `make measure-tools-timing` - **in that
# order**, because the second ends with `timing-report`, which reads both CSVs. What still has to
# be re-read by a human afterwards is the *prose*: it makes ordering claims ("no AST-aware tool
# beats plain diff", "fastest of the three at the median") that the macros cannot check, and a
# refresh can reverse them. Together, never piecemeal.
#
# Also does not regenerate plots/tips.png, which needs the full repository-fetch pipeline
# (`make full`). Needs a LaTeX toolchain with the acmart class and cm-super (see
# papers/introductory-paper/README.md). `-g` forces a full latexmk run regardless of its own
# up-to-date check: without it, latexmk can decide main.pdf is already current from main.tex's own
# timestamp alone and skip the rebuild even though an \input-ed generated table just changed
# underneath it, leaving main.pdf silently one run stale.
#
# `build-pdf` falls back to a manual pdflatex+bibtex sequence when latexmk is not installed,
# since a machine can have pdflatex without it. Three pdflatex passes is the standard fixed point
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
