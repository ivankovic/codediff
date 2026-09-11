# Sample provenance

- **Repository:** https://github.com/apache/commons-math.git (`apache-commons-math.git`)
- **Commit:** `f6dd42b4fc01d1bef5392b6f19b6bcbafee27637`
- **File:** `src/main/java/org/apache/commons/math/analysis/solvers/BrentSolver.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Math-73** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`05361c5a3409a9ca8e4e3438eb87d1cd55b713a8`) and `after.java.test` the fixed revision
(`f6dd42b4fc01d1bef5392b6f19b6bcbafee27637`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://issues.apache.org/jira/browse/MATH-343

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
