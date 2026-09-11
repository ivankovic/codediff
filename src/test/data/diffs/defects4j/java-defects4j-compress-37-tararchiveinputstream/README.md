# Sample provenance

- **Repository:** https://github.com/apache/commons-compress.git (`apache-commons-compress.git`)
- **Commit:** `19a620c904587dc0397b43bfe9071ff60033c097`
- **File:** `src/main/java/org/apache/commons/compress/archivers/tar/TarArchiveInputStream.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Compress-37** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`14b3d1ad8c5efc1f21c94e6d0f72635c374b2bb6`) and `after.java.test` the fixed revision
(`19a620c904587dc0397b43bfe9071ff60033c097`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://issues.apache.org/jira/browse/COMPRESS-355

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
