# Sample provenance

- **Repository:** https://github.com/apache/commons-compress.git (`apache-commons-compress.git`)
- **Commit:** `3c4a09bf28e7cd600b919b8c799fbbfd19a94c0b`
- **File:** `src/main/java/org/apache/commons/compress/archivers/tar/TarArchiveInputStream.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Compress-32** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`24a3100e9897837b513a0d9f2ae26fd02ec91246`) and `after.java.test` the fixed revision
(`3c4a09bf28e7cd600b919b8c799fbbfd19a94c0b`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://issues.apache.org/jira/browse/COMPRESS-314

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
