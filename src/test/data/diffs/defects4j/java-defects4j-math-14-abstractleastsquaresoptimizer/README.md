# Sample provenance

- **Repository:** https://github.com/apache/commons-math.git (`apache-commons-math.git`)
- **Commit:** `b07ecae3d60c4f0233f1a1d97eb35d4a678f39aa`
- **File:** `src/main/java/org/apache/commons/math3/optim/nonlinear/vector/jacobian/AbstractLeastSquaresOptimizer.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Math-14** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`ace42058d67a52fb362543c5cf5aca64cf8de3e8`) and `after.java.test` the fixed revision
(`b07ecae3d60c4f0233f1a1d97eb35d4a678f39aa`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://issues.apache.org/jira/browse/MATH-924

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
