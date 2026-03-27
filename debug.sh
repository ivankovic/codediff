#!/bin/bash
#
#  This file is part of the CodeDiff code diffing tool.
#
#  Copyright (C) 2025 Marko Ivankovic
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
MODE="dirs"   # default

# Parse optional flag
while [ $# -gt 1 ]; do
  case "$1" in
    --all) MODE="all" ;;     # files + directories
    --dirs) MODE="dirs" ;;   # directories only
    --repositories) MODE="repositories" ;;
    *) echo "unknown option: $1" >&2; exit 1 ;;
  esac
  shift
done

cargo build --release

# Target directory (current dir if not specified)
PARENT="$1"
DB_PATH=/var/tmp/research/debug-stats.sqlite3

case "$MODE" in
  repositories)
    for PATH_TO_PROCESS in "$PARENT"/*/; do
      [ -d "$PATH_TO_PROCESS" ] || continue
      echo "Repository: $PATH_TO_PROCESS"
      if ! ./target/release/commit_stats --path="$PATH_TO_PROCESS" --db="$DB_PATH" >/dev/null 2>&1; then
        echo "FAILED: $PATH_TO_PROCESS"
      fi
    done
    ;;
  dirs)
    for PATH_TO_PROCESS in "$PARENT"/*/; do
      [ -d "$PATH_TO_PROCESS" ] || continue
      echo "Directory: $PATH_TO_PROCESS"
      if ! ./target/release/file_stats --path="$PATH_TO_PROCESS" --db="$DB_PATH" >/dev/null 2>&1; then
        echo "FAILED: $PATH_TO_PROCESS"
      fi
    done
    ;;
  all)
    for PATH_TO_PROCESS in "$PARENT"/*; do
      [ -e "$PATH_TO_PROCESS" ] || continue
      if ! ./target/release/file_stats --path="$PATH_TO_PROCESS" --db="$DB_PATH" >/dev/null 2>&1; then
        echo "FAILED: $PATH_TO_PROCESS"
      fi
    done
    ;;
esac

