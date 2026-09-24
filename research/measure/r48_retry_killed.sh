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
# After an R48 run (measure/overnight_benchmarks.sh): give every pair that did not get a fair
# measurement a second one, then re-merge.
#
# Three groups, from the killed records (which carry the exit status the attempt died with) and
# the merged CSV:
#
#   134 / 143  SIGABRT / SIGTERM. An abort is the harness's diff thread running on a smaller
#              stack than the product's 256MB one and overflowing on files the product diffs; a
#              SIGTERM is an operator restarting a shard. Neither is a finding about the pair.
#              Re-measured under the ordinary cap.
#   137        SIGKILL, the shard's memory cap. These cluster in a few generated files (parse
#              tables, data arrays) at many commits, so one commit per distinct file is
#              re-measured under BIG_MEMORY_MAX, BIG_PARALLEL at a time, with a longer budget:
#              the row then says what that file needs, and one that dies again means "more than
#              BIG_MEMORY_MAX". The other commits of the same file keep their `killed` row.
#   timed_out  The 120-second budget was applied while eight shards shared the machine. Re-run
#              TIMEOUT_PARALLEL at a time; a pair that times out again did so on a quiet machine.
#
# Usage (from research/):  ./measure/r48_retry_killed.sh
#   OUT=/var/tmp/research/full/robustness   SHARDS=8
#   BIG_MEMORY_MAX=24G  BIG_PARALLEL=2  BIG_TIMEOUT=300  TIMEOUT_PARALLEL=4

set -uo pipefail
cd "$(dirname "$0")/.."

OUT="${OUT:-/var/tmp/research/full/robustness}"
REPOS="${REPOS:-/var/tmp/research/full/repositories}"
SHARDS="${SHARDS:-8}"
BIG_MEMORY_MAX="${BIG_MEMORY_MAX:-24G}"
BIG_PARALLEL="${BIG_PARALLEL:-2}"
BIG_TIMEOUT="${BIG_TIMEOUT:-300}"
TIMEOUT_PARALLEL="${TIMEOUT_PARALLEL:-4}"
STAMP="$(date +%Y%m%d%H%M%S)"

stage() { echo; echo "[$(date '+%Y-%m-%d %H:%M:%S')] $*"; }

# The pair lists for the three groups, in the shape benchmark_diff_pairs reads (old_path from
# the run's own list, so a rename keeps its before-side path).
stage "listing the pairs to retry"
python3 - "$OUT" "$SHARDS" <<'PY'
import csv, glob, sys
out, shards = sys.argv[1], int(sys.argv[2])
key = lambda r: (r["repository"], r["commit"], r["path"])
with open(f"{out}/code_edits_full.csv", newline="") as f:
    reader = csv.DictReader(f)
    fields = reader.fieldnames
    listed = {key(r): r for r in reader}
kills = [r for p in glob.glob(f"{out}/robustness_shard_*.killed.csv") for r in csv.DictReader(open(p, newline=""))]
small = [listed[key(r)] for r in kills if r["exit_status"] != "137" and key(r) in listed]
seen = set(); big = []
for r in kills:
    if r["exit_status"] == "137" and key(r) in listed and (r["repository"], r["path"]) not in seen:
        seen.add((r["repository"], r["path"])); big.append(listed[key(r)])
with open(f"{out}/robustness_full.csv", newline="") as f:
    timed = [listed[key(r)] for r in csv.DictReader(f) if r["status"] == "timed_out" and key(r) in listed]
for name, rows in (("small", small), ("big", big), ("timeouts", timed)):
    with open(f"{out}/retry_{name}.csv", "w", newline="") as f:
        w = csv.DictWriter(f, fieldnames=fields); w.writeheader(); w.writerows(rows)
    print(f"  {name}: {len(rows)} pairs")
PY

run() {  # run <csv> <output part> <MemoryMax> <timeout-secs>
  systemd-run --user --scope --quiet --collect --slice=codediff-r48.slice \
    -p MemoryMax="$3" -p MemorySwapMax=0 \
    ../target/release/benchmark_diff_pairs \
      --csv "$1" --repo-root "$REPOS" --output "$2" \
      --max-combined-nodes 1000000000 --timeout-secs "$4" --iterations 1 \
      >> "$OUT/retry_$STAMP.log" 2>&1
}

# Every retry writes a part named after this pass, sorted after the main run's parts, so the
# merge takes its row in place of the main run's for the same pair.
if [ "$(($(wc -l < "$OUT/retry_small.csv") - 1))" -gt 0 ]; then
  stage "re-measuring the aborted and restarted pairs under the ordinary cap"
  run "$OUT/retry_small.csv" "$OUT/robustness_shard_0.part9${STAMP}_small.csv" 6G 120 \
    || echo "  a pair aborted again on a 256MB stack - a real finding; see retry_$STAMP.log"
fi

if [ "$(($(wc -l < "$OUT/retry_timeouts.csv") - 1))" -gt 0 ]; then
  stage "re-running the timeouts $TIMEOUT_PARALLEL at a time"
  python3 - "$OUT/retry_timeouts.csv" "$TIMEOUT_PARALLEL" "$OUT/retry_timeouts_" <<'PY'
import csv, sys
src, n, prefix = sys.argv[1], int(sys.argv[2]), sys.argv[3]
rows = list(csv.DictReader(open(src, newline="")))
for i in range(n):
    with open(f"{prefix}{i}.csv", "w", newline="") as f:
        w = csv.DictWriter(f, fieldnames=list(rows[0]) if rows else ["language"]); w.writeheader(); w.writerows(rows[i::n])
PY
  for i in $(seq 0 $((TIMEOUT_PARALLEL - 1))); do
    run "$OUT/retry_timeouts_$i.csv" "$OUT/robustness_shard_$i.part9${STAMP}_timeouts.csv" 6G 120 &
  done
  wait
fi

if [ "$(($(wc -l < "$OUT/retry_big.csv") - 1))" -gt 0 ]; then
  stage "re-measuring one commit of each memory-killed file under $BIG_MEMORY_MAX, $BIG_PARALLEL at a time, ${BIG_TIMEOUT}s each"
  k=0
  while IFS= read -r row; do
    k=$((k + 1))
    one="$OUT/retry_big_$k.csv"
    { head -1 "$OUT/retry_big.csv"; echo "$row"; } > "$one"
    ( run "$one" "$OUT/robustness_shard_$((k % SHARDS)).part9${STAMP}_big$(printf '%03d' "$k").csv" "$BIG_MEMORY_MAX" "$BIG_TIMEOUT" \
        || echo "  still killed at $BIG_MEMORY_MAX: $(echo "$row" | cut -d, -f3,5)" ) &
    if [ $((k % BIG_PARALLEL)) -eq 0 ]; then wait; fi
  done < <(tail -n +2 "$OUT/retry_big.csv")
  wait
fi

stage "merging"
uv run ./analysis/robustness_merge.py --out "$OUT" --shards "$SHARDS"
stage "DONE"
