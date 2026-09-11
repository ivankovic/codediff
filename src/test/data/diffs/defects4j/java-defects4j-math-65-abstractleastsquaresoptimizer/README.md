# Sample provenance

- **Repository:** https://github.com/apache/commons-math.git (`apache-commons-math.git`)
- **Commit:** `c640932de203b33683679879cdf4b1b637d473d3`
- **File:** `src/main/java/org/apache/commons/math/optimization/general/AbstractLeastSquaresOptimizer.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Math-65** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`b3777d116ef961d0f1f3bb5575e3856668e48d81`) and `after.java.test` the fixed revision
(`c640932de203b33683679879cdf4b1b637d473d3`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://issues.apache.org/jira/browse/MATH-377

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
