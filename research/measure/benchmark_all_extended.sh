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
#  MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
#  GNU Affero General Public License for more details.
#
#  You should have received a copy of the GNU Affero General Public License
#  along with this program.  If not, see <https://www.gnu.org/licenses/>.
#
# Run sample_code_pairs + benchmark_diff_pairs for one or more languages and
# write canonical results to results/benchmark_<language>.csv.
#
# Extended version with more languages and crash protection.
#
# Must be run from research/ (the directory containing this script's parent).
#
# Usage:
#   ./measure/benchmark_all_extended.sh [OPTIONS]
#
# Options:
#   --language LANG       Language or 'all' (default: all)
#                       Supported: Rust, Python, Go, Kotlin, Java, JavaScript, 
#                       TypeScript, C, CPP, Ruby, PHP, Swift, Scala, Lua
#   --limit N             Max combined AST nodes to attempt (default: 20000)
#   --count N             Pairs to sample when no sample file exists (default: 1000)
#   --repos-dir DIR       Root of checked-out git repositories
#                       (default: /var/tmp/research/small/repositories/)
#   --bin-dir DIR         Directory containing binaries
#                       (default: ../target/release relative to research/)
#   --resample            Re-run sampling even if CSV exists
#   --results-dir DIR     Where to write benchmark_<language>.csv (default: results/)
#   --timeout-min N       Timeout per language in minutes (default: 60)
#   --continue-on-error   Continue to next language if one fails (default: off)
#   --max-commits N      Max commits per repo during sampling (default: 100)
#
# Example — re-run everything with the current binary:
#   cargo build --release && ./measure/benchmark_all_extended.sh
#
# Example — benchmark Java and JavaScript at higher limit:
#   ./measure/benchmark_all_extended.sh --language "Java JavaScript" --limit 30000

set -uo pipefail

# ── defaults ────────────────────────────────────────────────────────────────
LANGUAGE="all"
LIMIT=20000
COUNT=1000
REPOS_DIR="/var/tmp/research/small/repositories/"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RESEARCH_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
BIN_DIR="$RESEARCH_DIR/../target/release"
RESULTS_DIR="$RESEARCH_DIR/results"
RESAMPLE=0
TIMEOUT_MIN=60
CONTINUE_ON_ERROR=0
MAX_COMMITS_PER_REPO=100

# ── arg parse ───────────────────────────────────────────────────────────────
while [[ $# -gt 0 ]]; do
  case "$1" in
    --language)        LANGUAGE="$2";              shift 2 ;;
    --limit)          LIMIT="$2";                shift 2 ;;
    --count)          COUNT="$2";                shift 2 ;;
    --repos-dir)      REPOS_DIR="$2";            shift 2 ;;
    --bin-dir)        BIN_DIR="$2";              shift 2 ;;
    --results-dir)    RESULTS_DIR="$2";          shift 2 ;;
    --resample)       RESAMPLE=1;                  shift   ;;
    --timeout-min)    TIMEOUT_MIN="$2";          shift 2 ;;
    --continue-on-error) CONTINUE_ON_ERROR=1;     shift   ;;
    --max-commits)    MAX_COMMITS_PER_REPO="$2";  shift 2 ;;
    -h|--help)
      sed -n '/^# Usage:/,/^[^#]/{ /^[^#]/q; s/^# \?//; p }' "$0"
      exit 0 ;;
    *) echo "Unknown option: $1" >&2; exit 1 ;;
  esac
done

# ── supported languages ──────────────────────────────────────────────────────
ALL_LANGUAGES=("Rust" "Python" "Go" "Kotlin" "Java" "JavaScript" "TypeScript" "C" "CPP" "Ruby" "PHP" "Swift" "Scala" "Lua")

# ── resolve language list ────────────────────────────────────────────────────
if [[ "$LANGUAGE" == "all" ]]; then
  LANGUAGES=("${ALL_LANGUAGES[@]}")
else
  IFS=', ' read -ra LANGUAGES <<< "$LANGUAGE"
fi

# ── validate binaries ────────────────────────────────────────────────────────
SAMPLER="$BIN_DIR/sample_code_pairs"
BENCHER="$BIN_DIR/benchmark_diff_pairs"
for bin in "$SAMPLER" "$BENCHER"; do
  if [[ ! -x "$bin" ]]; then
    echo "Binary not found or not executable: $bin" >&2
    echo "Run 'cargo build --release' first, or set --bin-dir." >&2
    exit 1
  fi
done

mkdir -p "$RESULTS_DIR"

# ── timeout handler ──────────────────────────────────────────────────────────
TIMEOUT_SEC=$((TIMEOUT_MIN * 60))

run_with_timeout() {
  local lang="$1"
  local cmd="$2"
  local log_file="$RESULTS_DIR/benchmark_${lang,,}.log"
  
  echo "  [Timeout: ${TIMEOUT_MIN}min] Starting..."
  
  if timeout "$TIMEOUT_SEC" bash -c "$cmd" > "$log_file" 2>&1; then
    echo "  ✓ Completed successfully"
    return 0
  else
    local exit_code=$?
    echo "  ✗ Timed out or failed (exit code: $exit_code)"
    echo "  Log saved to: $log_file"
    tail -20 "$log_file" >&2
    return 1
  fi
}

# ── per-language loop ────────────────────────────────────────────────────────
TOTAL_LANGUAGES=${#LANGUAGES[@]}
SUCCESS_COUNT=0
FAIL_COUNT=0

for LANG in "${LANGUAGES[@]}"; do
  [[ -z "$LANG" ]] && continue
  
  LANG_LOWER="${LANG,,}"
  SAMPLE_CSV="$RESEARCH_DIR/sampled_code_pairs_${LANG_LOWER}.csv"
  RESULT_CSV="$RESULTS_DIR/benchmark_${LANG_LOWER}.csv"

  echo "═══════════════════════════════════════════════════════════════"
  echo "  Language : $LANG ($((SUCCESS_COUNT + FAIL_COUNT + 1))/$TOTAL_LANGUAGES)"
  echo "  Limit    : $LIMIT nodes"
  echo "  Count    : $COUNT pairs"
  echo "  Max commits per repo: $MAX_COMMITS_PER_REPO"
  echo "  Repos    : $REPOS_DIR"
  echo "  Output   : $RESULT_CSV"
  echo "═══════════════════════════════════════════════════════════════"

  # ── step 1: sample ──────────────────────────────────────────────────────
  if [[ ! -f "$SAMPLE_CSV" || "$RESAMPLE" -eq 1 ]]; then
    echo "[1/2] Sampling $COUNT pairs → $SAMPLE_CSV"
    
    SAMPLE_CMD="\"$SAMPLER\" \\
      --path \"$REPOS_DIR\" \\
      --output \"$SAMPLE_CSV\" \\
      --language \"$LANG\" \\
      --count \"$COUNT\" \\
      --max-commits-per-repo \"$MAX_COMMITS_PER_REPO\" \\
      --seed 42"
    
    if ! run_with_timeout "${LANG}_sample" "$SAMPLE_CMD"; then
      echo "  ✗ Sampling failed for $LANG"
      if [[ "$CONTINUE_ON_ERROR" -eq 1 ]]; then
        FAIL_COUNT=$((FAIL_COUNT + 1))
        continue
      else
        exit 1
      fi
    fi
    
    if [[ ! -f "$SAMPLE_CSV" ]] || [[ ! -s "$SAMPLE_CSV" ]]; then
      echo "  ✗ Sample file $SAMPLE_CSV is empty or missing"
      if [[ "$CONTINUE_ON_ERROR" -eq 1 ]]; then
        FAIL_COUNT=$((FAIL_COUNT + 1))
        continue
      else
        exit 1
      fi
    fi
    
    PAIR_COUNT=$(( $(wc -l < "$SAMPLE_CSV") - 1 ))
    echo "  ✓ Sampled $PAIR_COUNT pairs for $LANG"
  else
    EXISTING=$(( $(wc -l < "$SAMPLE_CSV") - 1 ))
    echo "[1/2] Reusing existing $SAMPLE_CSV ($EXISTING pairs)"
  fi

  # ── step 2: benchmark ───────────────────────────────────────────────────
  echo "[2/2] Benchmarking → $RESULT_CSV"
  
  BENCH_CMD="\"$BENCHER\" \\
    --csv \"$SAMPLE_CSV\" \\
    --repo-root \"$REPOS_DIR\" \\
    --output \"$RESULT_CSV\" \\
    --max-combined-nodes \"$LIMIT\" \\
    --iterations 3 \\
    --fast-threshold-ms 500 \\
    --timeout-secs 60"
  
  if ! run_with_timeout "${LANG}_benchmark" "$BENCH_CMD"; then
    echo "  ✗ Benchmarking failed for $LANG"
    if [[ "$CONTINUE_ON_ERROR" -eq 1 ]]; then
      FAIL_COUNT=$((FAIL_COUNT + 1))
      continue
    else
      exit 1
    fi
  fi
  
  if [[ ! -f "$RESULT_CSV" ]] || [[ ! -s "$RESULT_CSV" ]]; then
    echo "  ✗ Result file $RESULT_CSV is empty or missing"
    if [[ "$CONTINUE_ON_ERROR" -eq 1 ]]; then
      FAIL_COUNT=$((FAIL_COUNT + 1))
      continue
    else
      exit 1
    fi
  fi
  
  RESULT_COUNT=$(( $(wc -l < "$RESULT_CSV") - 1 ))
  echo "  ✓ Benchmarked $RESULT_COUNT pairs for $LANG"
  
  SUCCESS_COUNT=$((SUCCESS_COUNT + 1))
  echo ""
done

echo "═══════════════════════════════════════════════════════════════"
echo "  Done: $SUCCESS_COUNT succeeded, $FAIL_COUNT failed out of $TOTAL_LANGUAGES"
echo "  Results written to $RESULTS_DIR/"
echo "═══════════════════════════════════════════════════════════════"

if [[ "$SUCCESS_COUNT" -gt 0 ]]; then
  echo ""
  echo "Run analysis:"
  echo "  cd $RESEARCH_DIR && uv run ./analysis/benchmark_report.py"
fi

if [[ "$FAIL_COUNT" -gt 0 ]]; then
  echo ""
  echo "Failed languages have logs in: $RESULTS_DIR/benchmark_*.log"
  exit 1
fi

exit 0
