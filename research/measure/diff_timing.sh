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

function stats() {
  datasets=("gh-java" "gh-python" "defects4j" "bugsinpy")
  no_diff_count=0

  echo "dataset,project,commit,file,file_chars_before,file_chars_after,diff_output_chars,time" > /tmp/diff_benchmark.csv

  for dataset in "${datasets[@]}"; do
    if [[ "$1" != "all" && "$dataset" != *"$1"* ]]; then
      continue
    fi

    for project_dir in ./benches/datasets/gumtreediff/$dataset/after/*; do
      project=$(basename "$project_dir")
      if [[ "$2" != "all" && "$project" != *"$2"* ]]; then
        continue
      fi

      echo "Dataset: $dataset Project: $project"
      for commit_dir in $project_dir/*; do
        commit=$(basename "$commit_dir")
        if [[ "$3" != "all" && "$commit" != *"$3"* ]]; then
          continue
        fi

        echo "Commit: $commit"
        for file_after in $commit_dir/*; do
          f=$(basename $file_after)
          if [[ "$4" != "all" && "$f" != *"$4"* ]]; then
            continue
          fi
          file_before="${file_after/\/after\//\/before\/}"

          file_chars_before=$(wc --chars $file_before | cut -d " " -f 1)
          file_chars_after=$(wc --chars $file_after | cut -d " " -f 1)

          diff_output_chars=-1
          diff_time=-1

          case "$5" in
            diff)
              /usr/bin/time -f "%e" -o /tmp/code_diff_benchmark_timing -q diff $file_before $file_after >/tmp/code_diff_benchmark_diff
              ;;
            gumtree)
              /usr/bin/time -f "%e" -o /tmp/code_diff_benchmark_timing -q ./binaries/gumtree-4.0.0-beta4/bin/gumtree textdiff $file_before $file_after >/tmp/code_diff_benchmark_diff
              ;;
            *)
              echo "Unknown difftool specified..."
              exit 1
              ;;
          esac

          diff_output_chars=$(wc --chars /tmp/code_diff_benchmark_diff | cut -d " " -f 1)
          diff_time=$(cat /tmp/code_diff_benchmark_timing)

          echo "$dataset,$project,$commit,$f,$file_chars_before,$file_chars_after,$diff_output_chars,$diff_time" >> /tmp/diff_benchmark.csv
        done
      done
    done
  done
}

cmd="$1"
shift || true

case "$cmd" in
  stats)
    dataset="all"
    project="all"
    commit="all"
    file="all"
    tool="diff"

    while [[ $# -gt 0 ]]; do
      case "$1" in
        -d|--dataset)
          dataset="$2"
          shift 2
          ;;
        -p|--project)
          project="$2"
          shift 2
          ;;
        -c|--commit)
          commit="$2"
          shift 2
          ;;
        -f|--file)
          file="$2"
          shift 2
          ;;
        -t|--tool)
          tool="$2"
          shift 2
          ;;
        *)
          break
          ;;
      esac
    done

    stats "$dataset" "$project" "$commit" "$file" "$tool"
    ;;
  *)
    echo "Usage:"
    echo "  stats --dataset=<dataset> --project=<project> --commit=<commit> --file=<file> --tool={diff|gumtreediff}"
    exit 1
    ;;
esac
