# Sample provenance

- **Repository:** https://github.com/apache/commons-compress.git (`apache-commons-compress.git`)
- **Commit:** `5066e9aeb98f386b29a31cd4acb97aa43844cd30`
- **File:** `src/main/java/org/apache/commons/compress/archivers/zip/ZipArchiveEntry.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Compress-6** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`41aa509d836dcdd316a40e68680cc54e0f6c1e04`) and `after.java.test` the fixed revision
(`5066e9aeb98f386b29a31cd4acb97aa43844cd30`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://issues.apache.org/jira/browse/COMPRESS-94

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
