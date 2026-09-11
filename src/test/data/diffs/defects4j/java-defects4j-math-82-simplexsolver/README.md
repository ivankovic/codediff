# Sample provenance

- **Repository:** https://github.com/apache/commons-math.git (`apache-commons-math.git`)
- **Commit:** `38983e820763c882e778c7d5c68b673fb45a210e`
- **File:** `src/main/java/org/apache/commons/math/optimization/linear/SimplexSolver.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Math-82** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`dbdff0758b40601238e88b2cffbf7ceb58ed8977`) and `after.java.test` the fixed revision
(`38983e820763c882e778c7d5c68b673fb45a210e`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://issues.apache.org/jira/browse/MATH-288

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
