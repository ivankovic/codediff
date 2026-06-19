# Next features to implement

*  UI that shows the mapping
*  Add "Diff script" generation that can take the ASTDiff and make a "insert, move, update, delete"
   script out of it.

# Benchmarking

*  Add diff-cost benchmarking to diff.rs. The diff cost is the number of inserts + deletions + updates.

# Possible code health improvements

*  Make code.rs parse code in the from_string if possible, and then remove parsing from diff_code
   diff.rs
*  Make all handmade test diff directories in src/test/data/diffs start with the language name, e.g.
   not "hello-world-added-message" but "rust-hello-world-added-message" etc.

# Ideas
