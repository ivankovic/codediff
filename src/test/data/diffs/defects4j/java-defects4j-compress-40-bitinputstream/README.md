# Sample provenance

- **Repository:** https://github.com/apache/commons-compress.git (`apache-commons-compress.git`)
- **Commit:** `52dd5908e374973d69c51856b74d4d93d591c90a`
- **File:** `src/main/java/org/apache/commons/compress/utils/BitInputStream.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Compress-40** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`32c30f6f072ccfea6ead90f8eef0c205d88d00d3`) and `after.java.test` the fixed revision
(`52dd5908e374973d69c51856b74d4d93d591c90a`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://issues.apache.org/jira/browse/COMPRESS-363

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
