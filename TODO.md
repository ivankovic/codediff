# Next features to implement

*  Update doc-comments in diff.rs
*  Add RTED
*  Add "Diff script" generation that can take the ASTDiff and make a "insert, move, update, delete"
   script out of it.

# Benchmarking

*  Add diff-cost benchmarking to diff.rs. The diff cost is the number of inserts + deletions + updates.
*  Add time benchmarking to hash.rs

# Possible code health improvements

*  Check that we actually need Serialize on all structures. Why do we even have that?
*  Make code.rs parse code in the from_string if possible, and then remove parsing from diff_code in
   diff.rs

# Ideas

*  Stats show huge clusters of "add-only" commits. Make adds super fast.
