#!/usr/bin/env bash
#
# Fetch the replication package of Falleri & Martinez, "Fine-grained, accurate and scalable
# source differencing" (ICSE 2024) - Zenodo record 10474674, CC-BY-4.0.
#
# What we want from it is the `dataset/` folder: before/after file pairs for Defects4J (1046
# files), BugsInPy (643), gh-java (999) and gh-python (991), each laid out as
# `<dataset>/{before,after}/<project>/<bug or sha>/<path with / replaced by _>`. The Defects4J
# half is the source that the Alikhanifard & Tsantalis oracle's offsets index into (see
# fetch_astdiff_oracle.sh and README.md) - same flattened file names, same buggy=before /
# fixed=after orientation, verified byte-for-byte against the oracle's CompilationUnit spans.
#
# `analysis/` is kept too: `qualitative_experiment.csv` is their 100-case human study (two
# authors, a harmonised verdict, seven external participants, each choosing GumTree greedy vs
# simple vs neither), the only other human judgement of diff output we found in the literature.
# The bundled GumTree source tree and the participants' GUI are dropped - the former is on GitHub,
# the latter is 200 rendered HTML pages of no use to a scorer.
#
# Idempotent: skipped if $DEST already exists. The zip is kept next to the extraction so a re-run
# after a partial extraction does not download 54 MB again.

set -euo pipefail

RECORD=10474674
ZIP_URL="https://zenodo.org/records/$RECORD/files/gumtree_simple_replication_package.zip?download=1"
DEST=${DEST:-/var/tmp/research/external/gumtree-simple}
ZIP="$(dirname "$DEST")/gumtree_simple_replication_package.zip"

if [ -d "$DEST/dataset" ]; then
    echo "Already present: $DEST"
    exit 0
fi

mkdir -p "$(dirname "$DEST")"
if [ ! -s "$ZIP" ]; then
    echo "Downloading Zenodo record $RECORD to $ZIP"
    curl --fail --location --silent --show-error --output "$ZIP" "$ZIP_URL"
fi

tmp=$(mktemp -d "$(dirname "$DEST")/gumtree-simple.XXXXXX")
trap 'rm -rf "$tmp"' EXIT
unzip -q "$ZIP" \
    'gumtree_simple_replication_package/dataset/*' \
    'gumtree_simple_replication_package/analysis/*' \
    'gumtree_simple_replication_package/README.md' \
    -d "$tmp"
mv "$tmp/gumtree_simple_replication_package" "$DEST"
echo "https://doi.org/10.5281/zenodo.$RECORD" > "$DEST/FETCHED"

for d in defects4j bugsinpy gh-java gh-python; do
    echo "$d: $(find "$DEST/dataset/$d/before" -type f | wc -l) before-files"
done
