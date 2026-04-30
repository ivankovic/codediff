# Next features to implement

*  Diff format: edit script
*  Diff format: byte-range
*  UI that shows the mapping
*  Update doc-comments in diff.rs
*  Add "Diff script" generation that can take the ASTDiff and make a "insert, move, update, delete"
   script out of it.

# Benchmarking

*  Add diff-cost benchmarking to diff.rs. The diff cost is the number of inserts + deletions + updates.

# Possible code health improvements

*  diff.rs move code_diff to be a function on the Diff object.
*  Rename handmade_test_diffs to handmade_test_code_pairs
*  Add handmade_test_diffs that creates actual Diff objects from code pairs. The function should
accept bool params for each diff representation so that it can control how much of the diff is
created.
*  Make code.rs parse code in the from_string if possible, and then remove parsing from diff_code in
   diff.rs

# Ideas

*  Stats show huge clusters of "add-only" commits. Make adds super fast.
