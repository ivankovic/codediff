# Sample provenance

- **Repository:** https://github.com/apache/commons-codec.git (`apache-commons-codec.git`)
- **Commit:** `c82fe35c48bd0082c16644d16d82d1be79d6b9d1`
- **File:** `src/main/java/org/apache/commons/codec/binary/Base32.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Codec-16** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`38357dffa2dd39c91c4523beade08c12e8009acb`) and `after.java.test` the fixed revision
(`c82fe35c48bd0082c16644d16d82d1be79d6b9d1`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://issues.apache.org/jira/browse/CODEC-200

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
