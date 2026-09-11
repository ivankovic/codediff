# Sample provenance

- **Repository:** https://github.com/apache/commons-math.git (`apache-commons-math.git`)
- **Commit:** `8fe6a83eb60b7b16cac065b68e60c637d64d5932`
- **File:** `src/main/java/org/apache/commons/math/optimization/direct/MultiDirectional.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Math-84** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`9cb0ca6b0fa639d30d9070f422de5a59c61eb86c`) and `after.java.test` the fixed revision
(`8fe6a83eb60b7b16cac065b68e60c637d64d5932`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://issues.apache.org/jira/browse/MATH-283

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
