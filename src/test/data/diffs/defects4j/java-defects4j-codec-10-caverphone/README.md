# Sample provenance

- **Repository:** https://github.com/apache/commons-codec.git (`apache-commons-codec.git`)
- **Commit:** `41c68e9ef470696009d72133c7f05a20e2728e34`
- **File:** `src/java/org/apache/commons/codec/language/Caverphone.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Codec-10** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`2a8fd84f1f380fc472ecf415b771cb5fd789719b`) and `after.java.test` the fixed revision
(`41c68e9ef470696009d72133c7f05a20e2728e34`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://issues.apache.org/jira/browse/CODEC-117

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
