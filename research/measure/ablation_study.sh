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
#  MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
#  GNU Affero General Public License for more details.
#
#  You should have received a copy of the GNU Affero General Public License
#  along with this program.  If not, see <https://www.gnu.org/licenses/>.
#
# Leave-one-out ablation study over every diff-algorithm heuristic/pass (see
# `Diff::from_code_with_config` in src/diff.rs and the --solver-X/--no-solver-X flags on
# benchmark_optimal_solutions). For each pass, runs the optimal_solutions benchmark with just that
# one pass disabled (--no-solver-X) and every other pass left at its default (enabled), then
# compares the total mismatch count against an all-enabled baseline - the delta is that pass's
# measured contribution to accuracy on the fixture corpus.
#
# Usage: ./ablation_study.sh [output-dir]  (default output-dir: research/ablation)
# Can be run from anywhere - always operates relative to the repo root, two directories up from
# this script's own location (research/measure/).

set -uo pipefail
cd "$(dirname "$0")/../.."

OUT_DIR="${1:-research/ablation}"
mkdir -p "$OUT_DIR"

BIN=./target/release/benchmark_optimal_solutions

echo "Building benchmark_optimal_solutions (release)..."
# --features test-fixtures: benchmark_optimal_solutions needs codediff::test's fixture-loading
# helpers, gated behind this feature (see Cargo.toml's [features]) since it needs no git2/rusqlite.
if ! cargo build --release --features test-fixtures --bin benchmark_optimal_solutions; then
  echo "Build failed, aborting." >&2
  exit 1
fi

# Keep this list in sync with the --no-solver-X flags in src/bin/benchmark_optimal_solutions.rs
# (which in turn mirror HeuristicConfig's fields in src/diff.rs). Only 4 passes have their own
# on/off knob post-rework (TODO.md, 2026-07-17/18) - the seven-phase pipeline's other steps run
# unconditionally, so there's nothing left to ablate for them.
FLAGS=(
  solver-import-nodes
  solver-similar-flow-control
  solver-bottom-up-expansion
  solver-moved-subtrees
)

FAILED=()

echo "Running baseline (all heuristics enabled)..."
if ! "$BIN" --csv "$OUT_DIR/baseline.csv" > "$OUT_DIR/baseline.log" 2>&1; then
  echo "Baseline run FAILED - see $OUT_DIR/baseline.log" >&2
  exit 1
fi

for flag in "${FLAGS[@]}"; do
  echo "Running with --no-$flag..."
  if ! "$BIN" "--no-$flag" --csv "$OUT_DIR/$flag.csv" > "$OUT_DIR/$flag.log" 2>&1; then
    echo "  FAILED - see $OUT_DIR/$flag.log" >&2
    FAILED+=("$flag")
  fi
done

if [ ${#FAILED[@]} -gt 0 ]; then
  echo
  echo "${#FAILED[@]} run(s) failed to complete: ${FAILED[*]}"
  echo "(their .csv files are missing/stale - excluded from the summary below)"
fi

python3 - "$OUT_DIR" "${FLAGS[@]}" <<'PYEOF'
import csv
import os
import sys

out_dir = sys.argv[1]
flags = sys.argv[2:]


def totals(path):
    """(sum of mismatches over solved fixtures, count of unsolved fixtures)."""
    mismatches = 0
    unsolved = 0
    with open(path) as f:
        for row in csv.DictReader(f):
            if row["mismatches"] == "-":
                unsolved += 1
                continue
            mismatches += int(row["mismatches"])
    return mismatches, unsolved


baseline_path = os.path.join(out_dir, "baseline.csv")
if not os.path.exists(baseline_path):
    print("No baseline.csv - baseline run must have failed.")
    sys.exit(1)

baseline_total, baseline_unsolved = totals(baseline_path)

print()
print(f"{'Flag disabled':<45} {'Mismatches':>10} {'Delta':>8} {'Unsolved':>9}")
print("-" * 76)
print(f"{'(none - baseline, all enabled)':<45} {baseline_total:>10} {'':>8} {baseline_unsolved:>9}")

rows = []
for flag in flags:
    path = os.path.join(out_dir, f"{flag}.csv")
    if not os.path.exists(path):
        rows.append((flag, None, None, None))
        continue
    total, unsolved = totals(path)
    rows.append((flag, total, total - baseline_total, unsolved))

# Biggest accuracy contributors (largest regression when removed) first; failed runs last.
rows.sort(key=lambda r: (r[2] is None, -(r[2] or 0)))

for flag, total, delta, unsolved in rows:
    if total is None:
        print(f"{'--no-' + flag:<45} {'FAILED':>10} {'':>8} {'':>9}")
    else:
        print(f"{'--no-' + flag:<45} {total:>10} {delta:>+8} {unsolved:>9}")

print()
print(f"Per-run CSVs and logs written to {out_dir}/")
PYEOF
