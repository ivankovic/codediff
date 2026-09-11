# Sample provenance

- **Repository:** https://github.com/apache/commons-math.git (`apache-commons-math.git`)
- **Commit:** `24218a5278ca3d99c209efc990d16048e29c8536`
- **File:** `src/main/java/org/apache/commons/math/optimization/general/LevenbergMarquardtOptimizer.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Math-68** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`24e05bf7e94edc4eca020a685010246b8b7788d1`) and `after.java.test` the fixed revision
(`24218a5278ca3d99c209efc990d16048e29c8536`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://issues.apache.org/jira/browse/MATH-362

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
