# Sample provenance

- **Repository:** https://github.com/apache/commons-codec.git (`apache-commons-codec.git`)
- **Commit:** `b8c2d9d9dc9aab45f83cf49ac93cfa8546e4c08e`
- **File:** `src/java/org/apache/commons/codec/binary/Base64.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Codec-2** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`2f7454a01e4c2992bffc3d86137e632e80c5027f`) and `after.java.test` the fixed revision
(`b8c2d9d9dc9aab45f83cf49ac93cfa8546e4c08e`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://issues.apache.org/jira/browse/CODEC-77

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
