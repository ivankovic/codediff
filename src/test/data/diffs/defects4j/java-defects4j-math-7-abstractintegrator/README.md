# Sample provenance

- **Repository:** https://github.com/apache/commons-math.git (`apache-commons-math.git`)
- **Commit:** `424cbd201ca5969181d68cff99d8b9b77a41cefe`
- **File:** `src/main/java/org/apache/commons/math3/ode/AbstractIntegrator.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Math-7** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`d18a6b851035818e38637a7b2f60e6f5c6367480`) and `after.java.test` the fixed revision
(`424cbd201ca5969181d68cff99d8b9b77a41cefe`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://issues.apache.org/jira/browse/MATH-950

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
