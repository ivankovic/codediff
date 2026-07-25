#!/usr/bin/env bash
# This file is part of the CodeDiff code diffing tool.
#
# Copyright (C) 2026 Marko Ivankovic
#
# This program is free software: you can redistribute it and/or modify
# it under the terms of the GNU Affero General Public License as published
# by the Free Software Foundation, either version 3 of the License, or
# (at your option) any later version.
#
# This program is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
# GNU Affero General Public License for more details.
#
# You should have received a copy of the GNU Affero General Public License
# along with this program. If not, see <https://www.gnu.org/licenses/>.
#
# Compiles BatchDriver.java against an existing GumTree build's fat jar. Needs GUMTREE_BIN set the
# same way `benchmark_other` needs it (bin/gumtree from a built GumTree distribution) - the jar
# lives at ../lib/gumtree.jar relative to that script, per GumTree's own Gradle `application`
# plugin layout.
set -euo pipefail

: "${GUMTREE_BIN:?Set GUMTREE_BIN to a built GumTree bin/gumtree, e.g. .../bin/gumtree}"

driver_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
gumtree_jar="$(dirname "$(dirname "$GUMTREE_BIN")")/lib/gumtree.jar"
if [[ ! -f "$gumtree_jar" ]]; then
    echo "Expected GumTree's fat jar at $gumtree_jar (derived from GUMTREE_BIN=$GUMTREE_BIN) - not found." >&2
    exit 1
fi

javac -cp "$gumtree_jar" -d "$driver_dir/out" "$driver_dir/BatchDriver.java"
echo "Built $driver_dir/out/BatchDriver.class against $gumtree_jar"
