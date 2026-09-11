# Sample provenance

- **Repository:** https://github.com/apache/commons-codec.git (`apache-commons-codec.git`)
- **Commit:** `800f0531068ebaf2f2d257bb1bd805781ddd4760`
- **File:** `src/java/org/apache/commons/codec/binary/Base64.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Codec-5** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`4c6eea410d34d7ff4aa5c30fb7a2fa7c349dae18`) and `after.java.test` the fixed revision
(`800f0531068ebaf2f2d257bb1bd805781ddd4760`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://issues.apache.org/jira/browse/CODEC-98

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
