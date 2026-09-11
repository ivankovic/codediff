# Sample provenance

- **Repository:** https://github.com/apache/commons-compress.git (`apache-commons-compress.git`)
- **Commit:** `4becc32881aa983d69359b475892a4f2bc6e5b89`
- **File:** `src/main/java/org/apache/commons/compress/archivers/zip/ZipArchiveEntry.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Compress-15** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`306b3d6b551b2fc6a3c9729117d47f7d7dea9e89`) and `after.java.test` the fixed revision
(`4becc32881aa983d69359b475892a4f2bc6e5b89`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://issues.apache.org/jira/browse/COMPRESS-187

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
