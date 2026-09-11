# Sample provenance

- **Repository:** https://github.com/apache/commons-compress.git (`apache-commons-compress.git`)
- **Commit:** `6f379134ae1807cd404ed6c9579707e5dc6a6df0`
- **File:** `src/main/java/org/apache/commons/compress/utils/ChecksumCalculatingInputStream.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Compress-44** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`b53ead4b43c6c248b1a39f4a1cce7a0c4062285d`) and `after.java.test` the fixed revision
(`6f379134ae1807cd404ed6c9579707e5dc6a6df0`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://issues.apache.org/jira/browse/COMPRESS-412

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
