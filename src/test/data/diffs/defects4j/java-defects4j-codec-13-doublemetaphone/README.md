# Sample provenance

- **Repository:** https://github.com/apache/commons-codec.git (`apache-commons-codec.git`)
- **Commit:** `37ba197e62d6b60037d18afc33801e6221f1b8c6`
- **File:** `src/main/java/org/apache/commons/codec/language/DoubleMetaphone.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Codec-13** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`8c145775da55fb33104751199a28809acb657c1f`) and `after.java.test` the fixed revision
(`37ba197e62d6b60037d18afc33801e6221f1b8c6`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://issues.apache.org/jira/browse/CODEC-184

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
