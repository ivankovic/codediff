# Sample provenance

- **Repository:** https://github.com/apache/commons-codec.git (`apache-commons-codec.git`)
- **Commit:** `954d995c5603b616c3c4a9ffb1823f36dd7ebcb0`
- **File:** `src/java/org/apache/commons/codec/binary/Base64.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Codec-7** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`f7966c1b374ebdd3fccb28370d9cb80a2115d807`) and `after.java.test` the fixed revision
(`954d995c5603b616c3c4a9ffb1823f36dd7ebcb0`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://issues.apache.org/jira/browse/CODEC-99

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
