# Sample provenance

- **Repository:** https://github.com/apache/commons-compress.git (`apache-commons-compress.git`)
- **Commit:** `fdac1475918e93444b08a024d096b4ec39d6733d`
- **File:** `src/main/java/org/apache/commons/compress/archivers/tar/TarArchiveOutputStream.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Compress-9** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`ec0db741b7e53d6fbe3887f8720f76a625d2277f`) and `after.java.test` the fixed revision
(`fdac1475918e93444b08a024d096b4ec39d6733d`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://issues.apache.org/jira/browse/COMPRESS-160

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
