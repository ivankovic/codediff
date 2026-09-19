#!/usr/bin/env bash
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
# The two measurements the 2026-09-18 review left open, run back to back on an idle machine:
#
#   R22  RQ2 re-drawn at COUNT=1000 pairs per language and re-measured - the whole
#        overnight_rq1_refresh.sh chain with SKIP_FETCH=1 (the corpus on disk is already at
#        depth 50). Timing-sensitive: every pair is held to a 1-second wall-clock budget, so
#        nothing else may run on the machine during it. Four to six hours.
#   R48  Robustness over every modified code file in the Full corpus's recent history, not just
#        the fixture corpus: the only run on this machine that reaches files the size of the
#        Robust target's ceiling. Not timing-sensitive (it records timeouts and panics, with a
#        120-second budget per pair), so it runs as $SHARDS processes at once. Tens of CPU-hours.
#
# Order matters and is fixed: build, list the R48 pairs (a git-log walk, I/O only), R22 alone,
# then R48. The list is drawn first so that R48 can start the moment R22 ends, and the build is
# first so no compile competes with R22's clock.
#
# Usage (from research/), as its own systemd unit rather than a child of the shell:
#
#   systemd-run --user --unit codediff-overnight --collect --same-dir \
#     ./measure/overnight_benchmarks.sh [log-file]
#
# Why a unit: on 2026-09-19 one R48 shard hit a pair that needed more memory than the machine
# has, the kernel's OOM killer took it, and systemd then stopped the whole scope the shell lived
# in - the other seven shards, the orchestrator and the Claude Code session that had started it.
# A unit of its own is stopped alone. For the same reason every shard runs in its own transient
# scope with a hard memory cap (SHARD_MEMORY_MAX) below, so a pathological pair kills that shard's
# process and nothing else, and a killed shard resumes from its own output with the offending
# pair recorded as `killed` and skipped. `journalctl --user -u codediff-overnight` has the log.
#
# The shard scopes are not children of the unit (a scope cannot be), so they sit in a slice of
# their own, `codediff-r48.slice`, capped as a whole at SLICE_MEMORY_MAX. To stop everything:
#
#   systemctl --user stop codediff-overnight codediff-r48.slice
#
#   MODE=full        which corpus root under /var/tmp/research/ (both stages read the same one).
#   COUNT=1000       R22's pairs per language.
#   SHARDS=8         R48's concurrent processes.
#   SHARD_MEMORY_MAX=6G   cgroup memory cap per shard process (systemd MemoryMax syntax).
#   SLICE_MEMORY_MAX=48G  cap on all shards together, so they can never reach the system's
#                         OOM killer, which picks its own victims.
#   SKIP_R22=0       set to 1 to run R48 alone (e.g. after an R22 that already landed).
#   SKIP_LIST=0      set to 1 to reuse the pair list a previous run left under $OUT.
#   SKIP_SAMPLE=0    set to 1 to re-measure R22 against the sample already in data/samples/
#                    rather than drawing it again (stages 3 and 4 of overnight_rq1_refresh.sh
#                    alone). Sound only when that sample was drawn from the corpus on disk.
#
# Outputs. R22's land where overnight_rq1_refresh.sh puts them (data/rq1/, data/samples/, the
# paper). R48's are not committed - they are hundreds of thousands of rows reproducible from the
# clones - and go to /var/tmp/research/$MODE/robustness/: the pair list, one CSV per shard, and
# robustness_full.csv, the shards merged. Summarise from there; see data/performance/PROVENANCE.md.

set -uo pipefail
cd "$(dirname "$0")/.."

MODE="${MODE:-full}"
COUNT="${COUNT:-1000}"
SHARDS="${SHARDS:-8}"
SHARD_MEMORY_MAX="${SHARD_MEMORY_MAX:-6G}"
SLICE_MEMORY_MAX="${SLICE_MEMORY_MAX:-48G}"
SKIP_R22="${SKIP_R22:-0}"
SKIP_LIST="${SKIP_LIST:-0}"
SKIP_SAMPLE="${SKIP_SAMPLE:-0}"
ROOT="/var/tmp/research/$MODE"
REPOS="$ROOT/repositories"
OUT="$ROOT/robustness"
LAUNCH="$(date +%Y%m%d%H%M%S)"
LOG="${1:-$OUT/overnight_benchmarks_$(date +%Y%m%d_%H%M%S).log}"
mkdir -p "$OUT"
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

echo "Overnight benchmarks: MODE=$MODE COUNT=$COUNT SHARDS=$SHARDS SHARD_MEMORY_MAX=$SHARD_MEMORY_MAX SKIP_R22=$SKIP_R22 SKIP_LIST=$SKIP_LIST SKIP_SAMPLE=$SKIP_SAMPLE"
echo "Log: $LOG"
[ -d "$REPOS" ] || fail "no corpus under $REPOS"

stage "Stage 1/4: build the release binaries (before anything timed runs)"
make build || fail "build failed"

stage "Stage 2/4: list every modified code file in the $MODE corpus for R48"
PAIRS="$OUT/code_edits_$MODE.csv"
if [ "$SKIP_LIST" = 1 ] && [ -s "$PAIRS" ]; then
  echo "SKIP_LIST=1: reusing $PAIRS"
else
  uv run ./analysis/list_code_edits.py --repositories "$REPOS" --max-commits 50 --output "$PAIRS" \
    || fail "listing pairs failed"
fi
TOTAL=$(($(wc -l < "$PAIRS") - 1))
echo "$TOTAL pairs listed in $PAIRS"

if [ "$SKIP_R22" = 1 ]; then
  stage "Stage 3/4: R22 skipped (SKIP_R22=1)"
elif [ "$SKIP_SAMPLE" = 1 ]; then
  stage "Stage 3/4: R22 - re-measure RQ2 against the sample in data/samples/, serial, machine otherwise idle"
  uv run ./analysis/verify_sample.py --repo-root "$REPOS" data/samples/sampled_code_pairs_all.csv \
    || fail "the sample in data/samples/ does not fully resolve - draw it again (SKIP_SAMPLE=0)"
  make measure-apted-budget MODE="$MODE" || fail "measure-apted-budget measurement failed"
  make introductory-paper || fail "paper rebuild failed"
else
  stage "Stage 3/4: R22 - RQ2 at COUNT=$COUNT, serial, machine otherwise idle"
  COUNT="$COUNT" SKIP_FETCH=1 MODE="$MODE" ./measure/overnight_rq1_refresh.sh "$OUT/r22_$(date +%Y%m%d_%H%M%S).log" \
    || fail "R22 chain failed"
fi

stage "Stage 4/4: R48 - robustness over $TOTAL pairs in $SHARDS shards, $SHARD_MEMORY_MAX each"
# Header on every shard: benchmark_diff_pairs reads a headed CSV. Rows are dealt round-robin so
# no shard gets one repository's worth of huge files. Existing outputs are kept: a shard resumes
# from whatever its earlier attempts measured (see run_shard), so a restart of this script costs
# nothing already done.
python3 - "$PAIRS" "$OUT" "$SHARDS" <<'PY'
import csv, sys
pairs, out, shards = sys.argv[1], sys.argv[2], int(sys.argv[3])
with open(pairs, newline="") as f:
    reader = csv.reader(f)
    header = next(reader)
    writers, files = [], []
    for i in range(shards):
        fh = open(f"{out}/shard_{i}.csv", "w", newline="")
        files.append(fh)
        w = csv.writer(fh)
        w.writerow(header)
        writers.append(w)
    for n, row in enumerate(reader):
        writers[n % shards].writerow(row)
    for fh in files:
        fh.close()
PY

# One shard, to completion. Each attempt measures whatever `shard_$i.csv` still lacks in the
# shard's output parts and runs in its own transient scope under SHARD_MEMORY_MAX; when the
# attempt dies (the cgroup OOM kill, a panic the harness could not isolate, anything), the pair
# it was on - the first one still unmeasured - is written to `robustness_shard_$i.killed.csv`
# with status `killed` and the next attempt skips it. The attempt cap is against a harness bug
# that would kill every pair, not against real killed pairs, of which there can be many.
run_shard() {
  local i="$1" attempt=0
  local input="$OUT/shard_$i.csv" killed="$OUT/robustness_shard_$i.killed.csv"
  while :; do
    local remaining="$OUT/shard_$i.remaining.csv"
    local left
    left=$(python3 - "$input" "$OUT" "$i" "$remaining" "$killed" <<'PY'
import csv, glob, sys
inp, out, i, remaining, killed = sys.argv[1:6]
key = lambda r: (r["repository"], r["commit"], r["path"])
done = set()
for p in glob.glob(f"{out}/robustness_shard_{i}.part*.csv") + [killed]:
    try:
        with open(p, newline="") as f:
            done.update(key(r) for r in csv.DictReader(f))
    except FileNotFoundError:
        pass
with open(inp, newline="") as f:
    reader = csv.DictReader(f)
    rows = [r for r in reader if key(r) not in done]
    fields = reader.fieldnames
with open(remaining, "w", newline="") as f:
    w = csv.DictWriter(f, fieldnames=fields)
    w.writeheader()
    w.writerows(rows)
print(len(rows))
PY
)
    [ "$left" -gt 0 ] || { echo "shard $i: complete"; return 0; }
    attempt=$((attempt + 1))
    if [ "$attempt" -gt 500 ]; then
      echo "shard $i: giving up after $attempt attempts with $left pairs left"
      return 1
    fi
    # Named by launch time and attempt, never by attempt alone: a relaunch of this script starts
    # counting at 1 again and would otherwise overwrite an earlier launch's parts.
    local part="$OUT/robustness_shard_$i.part${LAUNCH}_$(printf '%04d' "$attempt").csv"
    echo "shard $i: attempt $attempt, $left pairs left"
    systemd-run --user --scope --quiet --collect --slice=codediff-r48.slice \
      -p MemoryMax="$SHARD_MEMORY_MAX" -p MemorySwapMax=0 \
      ../target/release/benchmark_diff_pairs \
        --csv "$remaining" --repo-root "$REPOS" --output "$part" \
        --max-combined-nodes 1000000000 --timeout-secs 120 --iterations 1 \
        >> "$OUT/shard_$i.log" 2>&1
    local status=$?
    if [ "$status" -ne 0 ]; then
      # The victim is the first remaining pair whose row the attempt never wrote.
      python3 - "$remaining" "$part" "$killed" "$status" <<'PY'
import csv, os, sys
remaining, part, killed, status = sys.argv[1:5]
key = lambda r: (r["repository"], r["commit"], r["path"])
done = set()
if os.path.exists(part):
    with open(part, newline="") as f:
        done.update(key(r) for r in csv.DictReader(f))
with open(remaining, newline="") as f:
    victim = next((r for r in csv.DictReader(f) if key(r) not in done), None)
if victim is None:
    sys.exit(0)
new = not os.path.exists(killed)
with open(killed, "a", newline="") as f:
    w = csv.writer(f)
    if new:
        w.writerow(["language", "size_bucket", "repository", "commit", "path", "status", "exit_status"])
    w.writerow([victim["language"], victim["size_bucket"], victim["repository"], victim["commit"], victim["path"], "killed", status])
print(f"killed (exit {status}): {victim['repository']}@{victim['commit'][:10]} {victim['path']}")
PY
    fi
  done
}

systemctl --user set-property codediff-r48.slice MemoryMax="$SLICE_MEMORY_MAX" MemorySwapMax=0 \
  || echo "WARNING: could not cap codediff-r48.slice; shards are still capped individually"
pids=()
for i in $(seq 0 $((SHARDS - 1))); do
  run_shard "$i" &
  pids+=($!)
done
status=0
for pid in "${pids[@]}"; do
  wait "$pid" || status=1
done
[ "$status" = 0 ] || echo "WARNING: at least one shard gave up; see $OUT/shard_*.log"

# Merge: every shard's parts, then the killed pairs as rows of the same shape (measurement
# columns empty, status `killed`), so a reader of robustness_full.csv sees them.
MERGED="$OUT/robustness_full.csv"
python3 - "$OUT" "$SHARDS" "$MERGED" <<'PY'
import csv, glob, os, sys
out, shards, merged = sys.argv[1], int(sys.argv[2]), sys.argv[3]
header = None
with open(merged, "w", newline="") as fo:
    w = None
    for i in range(shards):
        for p in sorted(glob.glob(f"{out}/robustness_shard_{i}.part*.csv")):
            with open(p, newline="") as f:
                r = csv.DictReader(f)
                if w is None:
                    header = r.fieldnames
                    w = csv.DictWriter(fo, fieldnames=header)
                    w.writeheader()
                w.writerows(r)
        killed = f"{out}/robustness_shard_{i}.killed.csv"
        if os.path.exists(killed) and w is not None:
            with open(killed, newline="") as f:
                for r in csv.DictReader(f):
                    w.writerow({k: r.get(k, "") for k in header})
PY
echo "$(($(wc -l < "$MERGED") - 1)) rows merged into $MERGED"
python3 - "$MERGED" <<'PY'
import collections, csv, sys
counts = collections.Counter()
biggest = (0, None)
for r in csv.DictReader(open(sys.argv[1], newline="")):
    counts[r["status"]] += 1
    nodes = int(r.get("ast_nodes_before") or 0) + int(r.get("ast_nodes_after") or 0)
    if nodes > biggest[0]:
        biggest = (nodes, f'{r["repository"]} {r["commit"][:10]} {r["path"]}')
print("status counts:", dict(counts))
print("largest pair by combined AST nodes:", biggest)
PY

stage "DONE"
echo "Re-read before quoting anything:"
echo "  * data/rq1/PROVENANCE.md and the RQ2 figure caption (R22 moved the sample)"
echo "  * data/performance/PROVENANCE.md - record this R48 run and where its CSV lives"
