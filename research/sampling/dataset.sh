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

# Clone depth, in commits from each remote branch tip. Overridable with --depth because the right
# value is a per-corpus tradeoff, not a constant: it bounds how far back sampling can reach, and
# `git fetch --depth=N` on an existing shallow clone *shortens* as well as deepens, so lowering it
# discards history that is already on disk.
#
# Why this became a flag (2026-08-20): the `full` corpus on disk had only ~10-20 commits per
# repository despite this script hardcoding 1000, and the committed sample CSVs referenced commits
# that no checkout could resolve - roughly 41% of sampled pairs were unreadable, concentrated in
# whole repositories rather than spread evenly. A sample is only reproducible while the history it
# points into still exists, so record the depth each corpus was fetched at.
DEPTH="${DEPTH:-1000}"

function update() {
  rm -f "$2"/failed

  while IFS=, read -r project_name repository category; do
    if [[ "$1" != "all" && "$project_name" != *"$1"* ]]; then
      continue
    fi
    repository="$(echo "$repository" | xargs)"
    echo $project_name, $repository
    project_path="$2"/"$project_name"

    case "$repository" in
      https://github.com/* | https://gitlab.com/* | https://codeberg.org/*)
        # Strip the host prefix
        if [[ "$repository" == https://github.com/* ]]; then
          REST="${repository#https://github.com/}"
        elif [[ "$repository" == https://gitlab.com/* ]]; then
          REST="${repository#https://gitlab.com/}"
        else
          REST="${repository#https://codeberg.org/}"
        fi

        USER="$(echo "$REST" | cut -d/ -f1)"
        REPO="$(echo "$REST" | cut -d/ -f2)"

        if [ -n "$USER" ] && [ -n "$REPO" ]; then
          project_path="$2"/"$USER"-"$REPO"
        else
          echo "$project_name" >> "$2"/failed
          continue
        fi
        ;;
      *)
        project_path="$2"/"$project_name"
    esac

    if [ -d "$project_path" ]; then
      # Subshell, not a bare `cd`: this loop resolves `$2`/`$3` as relative paths on later
      # iterations otherwise, since a bare `cd` here leaks into every subsequent repository.
      ( cd "$project_path" && git fetch --depth="$DEPTH" ) || echo "$project_name" >> "$2"/failed
    else
      GIT_ASKPASS=true git clone --depth="$DEPTH" "$repository" "$project_path"
      if [ $? -ne 0 ]; then
        echo "$project_name" >> "$2"/failed
      fi
    fi
  done < <(tail -n +2 "$3")
}

cmd="$1"
shift || true

case "$cmd" in
  update)
    PROJECT_FILTER="all"
    ROOT_FOLDER=""
    LIST=""

    while [[ $# -gt 0 ]]; do
      case "$1" in
        -p|--project)
          PROJECT_FILTER="$2"
          shift 2
          ;;
        -r|--root)
          ROOT_FOLDER="$2"
          shift 2
          ;;
        -l|--list)
          LIST="$2"
          shift 2
          ;;
        -d|--depth)
          DEPTH="$2"
          shift 2
          ;;
        *)
          break
          ;;
      esac
    done

    # Check if required parameters are provided
    if [[ -z "$ROOT_FOLDER" ]]; then
      echo "Error: --root parameter is required"
      echo "Usage:"
      echo "  update --project <filter> --root <folder> --list <csv> [--depth N]"
      exit 1
    fi

    if [[ -z "$LIST" ]]; then
      echo "Error: --list parameter is required"
      echo "Usage:"
      echo "  update --project <filter> --root <folder> --list <csv> [--depth N]"
      exit 1
    fi

    update "$PROJECT_FILTER" "$ROOT_FOLDER" "$LIST"
    ;;
  *)
    echo "Usage:"
    echo "  update --project_filter <filter> --root_folder <folder> --list <csv> [--depth N]"
    exit 1
    ;;
esac
