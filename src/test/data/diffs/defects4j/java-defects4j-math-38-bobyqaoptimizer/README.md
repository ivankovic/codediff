# Sample provenance

- **Repository:** https://github.com/apache/commons-math.git (`apache-commons-math.git`)
- **Commit:** `0093f7b0a2a9e06ec5b5e1e551676269171c3e3d`
- **File:** `src/main/java/org/apache/commons/math/optimization/direct/BOBYQAOptimizer.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Math-38** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`d277738885e78a9339f12ebf10c3c26eb538c1cf`) and `after.java.test` the fixed revision
(`0093f7b0a2a9e06ec5b5e1e551676269171c3e3d`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://issues.apache.org/jira/browse/MATH-728

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
