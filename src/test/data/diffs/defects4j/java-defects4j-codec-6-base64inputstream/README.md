# Sample provenance

- **Repository:** https://github.com/apache/commons-codec.git (`apache-commons-codec.git`)
- **Commit:** `d74fc31604c805a47c44d7853f63a3b06ad6c016`
- **File:** `src/java/org/apache/commons/codec/binary/Base64InputStream.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Codec-6** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`5a0d6b1d2b38a8026b1e160b0de9d9d56b07665c`) and `after.java.test` the fixed revision
(`d74fc31604c805a47c44d7853f63a3b06ad6c016`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://issues.apache.org/jira/browse/CODEC-101

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
