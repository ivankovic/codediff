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

function update() {
  rm "$2"/failed

  while IFS=, read -r project_name repository category; do
    if [[ "$1" != "all" && "$dataset" != *"$1"* ]]; then
      continue
    fi
    repository="$(echo "$repository" | xargs)"
    echo $project_name, $repository
    project_path="$2"/"$project_name"

    case "$repository" in
      https://github.com/* | https://gitlab.com/* | https://codeberg.org/*)
        # Identify domain and strip prefix
        if [[ "$repository" == https://github.com/* ]]; then
          REST="${repository#https://github.com/}"
          DOMAIN="https://github.com"
        elif [[ "$repository" == https://gitlab.com/* ]]; then
          REST="${repository#https://gitlab.com/}"
          DOMAIN="https://gitlab.com"
        else
          REST="${repository#https://codeberg.org/}"
          DOMAIN="https://codeberg.org"
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
      cd "$project_path"
      git fetch --depth=10
    else
      GIT_ASKPASS=true git clone --depth=100 "$repository" "$project_path"
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
        *)
          break
          ;;
      esac
    done

    # Check if required parameters are provided
    if [[ -z "$ROOT_FOLDER" ]]; then
      echo "Error: --root parameter is required"
      echo "Usage:"
      echo "  update --project <filter> --root <folder> --list <csv>"
      exit 1
    fi

    if [[ -z "$LIST" ]]; then
      echo "Error: --list parameter is required"
      echo "Usage:"
      echo "  update --project <filter> --root <folder> --list <csv>"
      exit 1
    fi

    update "$PROJECT_FILTER" "$ROOT_FOLDER" "$LIST"
    ;;
  *)
    echo "Usage:"
    echo "  update --project_filter <filter> --root_folder <folder> --list <csv>"
    exit 1
    ;;
esac
