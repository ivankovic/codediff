# Sample provenance

- **Repository:** https://github.com/apache/commons-jxpath.git (`apache-commons-jxpath.git`)
- **Commit:** `716c03b3b12ec106974898451b149f6eb79c65da`
- **File:** `src/java/org/apache/commons/jxpath/ri/compiler/Expression.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **JxPath-2** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`5eb29ba7901ab88e8388095a8696c4ae13a2c163`) and `after.java.test` the fixed revision
(`716c03b3b12ec106974898451b149f6eb79c65da`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://issues.apache.org/jira/browse/JXPATH-50

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
