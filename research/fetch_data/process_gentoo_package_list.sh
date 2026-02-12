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

while read -r LINE; do
    case "$LINE" in
        https://github.com/* | https://gitlab.com/* | https://codeberg.org/*)
            # Identify domain and strip prefix
            if [[ "$LINE" == https://github.com/* ]]; then
                REST="${LINE#https://github.com/}"
                DOMAIN="https://github.com"
            elif [[ "$LINE" == https://gitlab.com/* ]]; then
                REST="${LINE#https://gitlab.com/}"
                DOMAIN="https://gitlab.com"
            else
                REST="${LINE#https://codeberg.org/}"
                DOMAIN="https://codeberg.org"
            fi

            # Extract USER and REPO (first two path components)
            USER="$(echo "$REST" | cut -d/ -f1)"
            REPO="$(echo "$REST" | cut -d/ -f2)"

            if [ -n "$USER" ] && [ -n "$REPO" ]; then
              echo "$REPO,$DOMAIN/$USER/$REPO,Uncategorized (Gentoo Package List)"
            fi
            ;;
    esac
done | sort -u
