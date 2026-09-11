#!/usr/bin/env bash
#
# Fetch Defects4J's per-project bug tables (`framework/projects/<Project>/active-bugs.csv` and
# `deprecated-bugs.csv` in https://github.com/rjust/defects4j): one row per bug with the buggy
# and fixed revision ids and the original bug-report URL. Both tables, because the AST-diff
# oracle and the replication package were built against an older Defects4J than today's master,
# which has since deprecated some of their bugs (Lang-18 was the first one hit) - a deprecated
# bug keeps its row, just in the other file.
#
# The oracle and the replication package both key Defects4J cases as `<Project>-<BugId>` and say
# nothing about which revision that is; these tables are what turns `Closure-157` back into a
# commit and a bug report, and `extract_defects4j_fixtures.py` writes both into each promoted
# fixture's README.
#
# The revision ids are Defects4J's, from the repositories it bundles (several were converted from
# SVN), not necessarily the upstream project's GitHub history - the README says so.
#
# 34 small CSVs, ~70 KB total. Idempotent: an existing file is not re-fetched.

set -euo pipefail

DEST=${DEST:-/var/tmp/research/external/defects4j-meta}
BASE=https://raw.githubusercontent.com/rjust/defects4j/master/framework/projects
PROJECTS="Chart Cli Closure Codec Collections Compress Csv Gson JacksonCore JacksonDatabind JacksonXml Jsoup JxPath Lang Math Mockito Time"

mkdir -p "$DEST"
fetched=0
for project in $PROJECTS; do
    for table in active deprecated; do
        # `<Project>.csv` for the active table (the name the first version of this script used),
        # `<Project>.deprecated.csv` for the other.
        file="$DEST/$project.csv"
        [ "$table" = deprecated ] && file="$DEST/$project.deprecated.csv"
        if [ -s "$file" ]; then
            continue
        fi
        curl --fail --location --silent --show-error --output "$file" \
            "$BASE/$project/$table-bugs.csv"
        fetched=$((fetched + 1))
    done
done
echo "https://github.com/rjust/defects4j framework/projects/*/{active,deprecated}-bugs.csv" > "$DEST/FETCHED"
echo "Defects4J metadata in $DEST: $fetched fetched, $(ls "$DEST"/*.csv | wc -l) tables present"
