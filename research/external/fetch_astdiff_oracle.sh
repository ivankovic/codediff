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
# Fetch the Alikhanifard & Tsantalis AST node-mapping benchmark (TOSEM 2025, arXiv 2403.05939).
#
# The ground truth is not a separate release: it is checked into the RefactoringMiner repository
# under src/test/resources/astDiff/{defects4j,commits}, ~2.2 GB of JSON. Everything else in that
# repository (the tool itself, 100+ MB of other test resources) is irrelevant here, so this is a
# depth-1, blob-filtered, sparse checkout of exactly those two directories. GitHub does not serve
# `git archive`, so a clone is the only way to get a subdirectory without the whole tarball.
#
# Idempotent: an existing checkout is left alone (re-run with REFRESH=1 to `git pull` it). The
# commit fetched is recorded in $DEST/FETCHED so a measurement can name the oracle version it was
# taken against - the checkout has grown since the paper (296 commit directories against the
# paper's 188), so "the benchmark" is not one fixed thing.
#
# See README.md in this directory for what the data is and how it is scored.

set -euo pipefail

REPO=https://github.com/tsantalis/RefactoringMiner.git
DEST=${DEST:-/var/tmp/research/external/refactoringminer-astdiff}
SPARSE_DIRS="src/test/resources/astDiff/defects4j src/test/resources/astDiff/commits"

if [ -d "$DEST/.git" ]; then
    if [ "${REFRESH:-0}" = "1" ]; then
        echo "Refreshing $DEST"
        git -C "$DEST" pull --ff-only
    else
        echo "Already present: $DEST ($(cat "$DEST/FETCHED" 2>/dev/null || echo 'commit unrecorded'))"
        echo "REFRESH=1 to pull."
        exit 0
    fi
else
    mkdir -p "$(dirname "$DEST")"
    echo "Cloning $REPO (sparse: $SPARSE_DIRS) into $DEST"
    git clone --quiet --depth 1 --filter=blob:none --sparse "$REPO" "$DEST"
    # shellcheck disable=SC2086
    git -C "$DEST" sparse-checkout set $SPARSE_DIRS
fi

commit=$(git -C "$DEST" rev-parse HEAD)
date=$(git -C "$DEST" log -1 --format=%cs)
echo "$commit $date" > "$DEST/FETCHED"

d4j=$(find "$DEST/src/test/resources/astDiff/defects4j" -mindepth 2 -maxdepth 2 -type d | wc -l)
commits=$(find "$DEST/src/test/resources/astDiff/commits" -mindepth 2 -maxdepth 2 -type d | wc -l)
echo "Fetched RefactoringMiner@$commit ($date): $d4j Defects4J cases, $commits refactoring commits"
