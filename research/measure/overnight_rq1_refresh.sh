#!/bin/bash
#
#  This file is part of the CodeDiff code diffing tool.
#
#  Copyright (C) 2026 Marko Ivankovic
#
#  This program is free software: you can redistribute it and/or modify
#  it under the terms of the GNU Affero General Public License as published
#  by the Free Software Foundation, either version 3 of the License, or
#  (at your option) any later version.
#
#  This program is distributed in the hope that it will be useful,
#  but WITHOUT ANY WARRANTY; without even the implied warranty of
#  MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
#  GNU Affero General Public License for more details.
#
#  You should have received a copy of the GNU Affero General Public License
#  along with this program. If not, see <https://www.gnu.org/licenses/>.
#
# One unattended run of the whole RQ1 chain: deepen the full corpus, re-draw the sampled pairs
# against it, re-measure RQ1, and rebuild the paper. Written to be started once and left alone.
#
# Why this exists as a script rather than a sequence of make invocations: the four stages must run
# strictly serially and each one's output is the next one's input, but only stage 3 is
# timing-sensitive. Interleaving them by hand is how a timing measurement ends up sharing a machine
# with a 7,445-repository git fetch.
#
# Usage (from research/):  ./measure/overnight_rq1_refresh.sh [log-file]
#
# Environment knobs, both defaulting to what the committed measurement used:
#
#   COUNT=1000       pairs per language, split over stats::sampling::LOC_BUCKETS' 7 buckets.
#   SKIP_FETCH=1     skip stage 1. Legitimate only when the corpus on disk is already at DEPTH and
#                    the sample is being re-drawn (stage 2) rather than re-measured: a fresh draw
#                    reads the checkouts themselves, so every pair it names resolves by
#                    construction, and stage 2b still proves it. Never skip it when re-measuring an
#                    existing sample, which is the decay case this script was written for.
#
# THE PROBLEM THIS FIXES (2026-08-20). The committed data/samples/sampled_code_pairs_*.csv named
# (repository, commit, path) triples that no checkout on this machine could resolve: ~41% of pairs
# failed to read, and the failures were concentrated in whole repositories rather than spread
# evenly, so a re-measurement would have silently dropped entire projects from RQ1. The cause is
# that a sample holds pointers, not blobs, while the clones it points into are shallow and keep
# being re-fetched. Deepening the corpus first, then sampling against what is actually on disk,
# is what makes the sample resolvable; it does not make it permanent, so re-sample rather than
# re-measure whenever the unreadable fraction climbs again.

set -uo pipefail
cd "$(dirname "$0")/.."

LOG="${1:-/var/tmp/rq1_overnight_$(date +%Y%m%d_%H%M%S).log}"
MODE=full
DEPTH=50
# Pairs per language, split over stats::sampling::LOC_BUCKETS' 7 buckets. Was a hardcoded 140 (20
# per bucket) until 2026-09-18, when the paper review asked for 1000 per language - roughly 24,000
# pairs over the corpus's 24 languages, against the 2,922 the committed RQ1 numbers are measured on.
COUNT="${COUNT:-140}"
SKIP_FETCH="${SKIP_FETCH:-0}"
REPOS=/var/tmp/research/$MODE/repositories

exec > >(tee -a "$LOG") 2>&1

stage() {
  echo
  echo "=============================================================================="
  echo "[$(date '+%Y-%m-%d %H:%M:%S')] $*"
  echo "=============================================================================="
}

fail() {
  echo "[$(date '+%Y-%m-%d %H:%M:%S')] ABORTED: $*" >&2
  exit 1
}

echo "RQ1 overnight refresh: MODE=$MODE DEPTH=$DEPTH COUNT=$COUNT"
echo "Log: $LOG"

# ── Stage 1: deepen the corpus ──────────────────────────────────────────────────────────────────
# Network-bound and serial, ~2.1s/repository measured against this corpus, so roughly 4-5 hours
# for 7,445. Deliberately not parallelized: the failure bookkeeping in dataset.sh appends to one
# `failed` file with no locking, and a half-corrupted record of which repositories failed is worse
# than a slower run that finishes before the sampling stage needs it.
stage "Stage 1/4: fetch $MODE corpus at --depth=$DEPTH"
if [ "$SKIP_FETCH" = 1 ]; then
  echo "SKIP_FETCH=1: leaving the corpus on disk as it is. Stage 2 draws from these checkouts, so"
  echo "the sample resolves by construction; stage 2b still checks."
else
  make fetch MODE=$MODE DEPTH=$DEPTH || fail "fetch failed"
fi
if [ -s "$REPOS/failed" ]; then
  echo "NOTE: $(wc -l < "$REPOS/failed") repositories failed to fetch (see $REPOS/failed)."
  echo "Not fatal: sampling draws from whatever resolved, and a repository absent from the corpus"
  echo "is a smaller corpus, not a wrong measurement."
fi

# ── Stage 2: re-draw the sample ─────────────────────────────────────────────────────────────────
# One sampling pass for every language, then split by the output's own `language` column - never
# one --language run per language, which re-walks every repository's history once per language.
stage "Stage 2/4: sample $COUNT pairs/language ($((COUNT / 7))/bucket) from the deepened corpus"
make sample-pairs-all MODE=$MODE COUNT=$COUNT || fail "sampling failed"

# The whole point of the exercise: every sampled pair must actually resolve against the checkouts
# it was drawn from. A sample that does not is the bug this script exists to fix, so verify it here
# rather than discovering it hours later in the RQ1 output's failed_to_read tally.
stage "Stage 2b/4: verify the new sample resolves"
uv run ./analysis/verify_sample.py --repo-root "$REPOS" data/samples/sampled_code_pairs_all.csv \
  || fail "the freshly drawn sample does not fully resolve - do not measure against it"

# ── Stage 3: re-measure RQ1 ─────────────────────────────────────────────────────────────────────
# TIMING-SENSITIVE: every pair gets a hard 1s budget enforced by killing a worker subprocess, so
# anything else competing for CPU turns pairs that would have finished into spurious timeouts.
# Stages 1 and 2 are finished by now, which is the reason they are stages rather than background
# jobs.
stage "Stage 3/4: re-measure RQ1 (serial, wall-clock against a 1s budget)"
make measure-apted-budget MODE=$MODE || fail "measure-apted-budget measurement failed"

# ── Stage 4: regenerate the paper ───────────────────────────────────────────────────────────────
# `make measure-apted-budget` already ran apted-budget-report, which writes plots/variables_rq1.tex; this folds it into
# plots/variables.tex and rebuilds the PDF.
stage "Stage 4/4: regenerate paper variables and rebuild the PDF"
make introductory-paper || fail "paper rebuild failed"

stage "DONE"
echo "Re-read before quoting anything:"
echo "  * data/rq1/PROVENANCE.md  - record DEPTH=$DEPTH and this run's date"
echo "  * the RQ1 figure's own caption - its attempted/measured counts moved"
echo "  * Section 3's claim about how the corpus was cloned - it is depth-limited, not full"
