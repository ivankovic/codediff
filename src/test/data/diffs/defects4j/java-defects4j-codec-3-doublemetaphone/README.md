# Sample provenance

- **Repository:** https://github.com/apache/commons-codec.git (`apache-commons-codec.git`)
- **Commit:** `a5dfe5cbc95d7f3ce0b4829756690c2cb8439f4c`
- **File:** `src/java/org/apache/commons/codec/language/DoubleMetaphone.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Codec-3** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`d7b01850ef418b1d261cdb2029f8f85ab4031553`) and `after.java.test` the fixed revision
(`a5dfe5cbc95d7f3ce0b4829756690c2cb8439f4c`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://issues.apache.org/jira/browse/CODEC-84

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
