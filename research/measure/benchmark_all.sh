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
# Must be run from research/ (the directory containing this script's parent).
#
# Usage:
#   ./measure/benchmark_all.sh [OPTIONS]
#
# Options:
#   --language LANG       Rust | Python | Go | Kotlin | all  (default: all)
#   --limit N             Max combined AST nodes to attempt (default: 16000)
#   --count N             Pairs to sample when no sample file exists (default: 1000)
#   --repos-dir DIR       Root of checked-out git repositories
#                         (default: /var/tmp/research/small/repositories/)
#   --bin-dir DIR         Directory containing sample_code_pairs and
#                         benchmark_diff_pairs binaries
#                         (default: ../target/release relative to research/)
#   --resample            Re-run sampling even if sampled_code_pairs_<lang>.csv exists
#   --results-dir DIR     Where to write benchmark_<language>.csv (default: results/)
#
# Example — re-run everything with the current binary:
#   cargo build --release && ./measure/benchmark_all.sh
#
# Example — benchmark Go only at a higher node limit:
#   ./measure/benchmark_all.sh --language Go --limit 32000

set -euo pipefail

# ── defaults ────────────────────────────────────────────────────────────────
LANGUAGE="all"
LIMIT=16000
COUNT=1000
REPOS_DIR="/var/tmp/research/small/repositories/"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RESEARCH_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
BIN_DIR="$RESEARCH_DIR/../target/release"
RESULTS_DIR="$RESEARCH_DIR/results"
RESAMPLE=0

# ── arg parse ───────────────────────────────────────────────────────────────
while [[ $# -gt 0 ]]; do
  case "$1" in
    --language)   LANGUAGE="$2";   shift 2 ;;
    --limit)      LIMIT="$2";      shift 2 ;;
    --count)      COUNT="$2";      shift 2 ;;
    --repos-dir)  REPOS_DIR="$2";  shift 2 ;;
    --bin-dir)    BIN_DIR="$2";    shift 2 ;;
    --results-dir) RESULTS_DIR="$2"; shift 2 ;;
    --resample)   RESAMPLE=1;      shift   ;;
    -h|--help)
      sed -n '/^# Usage:/,/^[^#]/{ /^[^#]/q; s/^# \?//; p }' "$0"
      exit 0 ;;
    *) echo "Unknown option: $1" >&2; exit 1 ;;
  esac
done

# ── resolve language list ────────────────────────────────────────────────────
if [[ "$LANGUAGE" == "all" ]]; then
  LANGUAGES=("Rust" "Python" "Go" "Kotlin")
else
  LANGUAGES=("$LANGUAGE")
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

# ── per-language loop ────────────────────────────────────────────────────────
for LANG in "${LANGUAGES[@]}"; do
  LANG_LOWER="${LANG,,}"
  SAMPLE_CSV="$RESEARCH_DIR/sampled_code_pairs_${LANG_LOWER}.csv"
  RESULT_CSV="$RESULTS_DIR/benchmark_${LANG_LOWER}.csv"

  echo "════════════════════════════════════════"
  echo "  Language : $LANG"
  echo "  Limit    : $LIMIT nodes"
  echo "  Repos    : $REPOS_DIR"
  echo "════════════════════════════════════════"

  # ── step 1: sample ──────────────────────────────────────────────────────
  if [[ ! -f "$SAMPLE_CSV" || "$RESAMPLE" -eq 1 ]]; then
    echo "[1/2] Sampling $COUNT pairs → $SAMPLE_CSV"
    "$SAMPLER" \
      --path "$REPOS_DIR" \
      --output "$SAMPLE_CSV" \
      --language "$LANG" \
      --count "$COUNT"
  else
    EXISTING=$(( $(wc -l < "$SAMPLE_CSV") - 1 ))
    echo "[1/2] Reusing existing $SAMPLE_CSV ($EXISTING pairs)"
  fi

  # ── step 2: benchmark ───────────────────────────────────────────────────
  echo "[2/2] Benchmarking → $RESULT_CSV"
  "$BENCHER" \
    --csv "$SAMPLE_CSV" \
    --repo-root "$REPOS_DIR" \
    --output "$RESULT_CSV" \
    --max-combined-nodes "$LIMIT"

  echo ""
done

echo "Done. Results written to $RESULTS_DIR/"
echo "Run:  uv run ./analysis/benchmark_report.py"
