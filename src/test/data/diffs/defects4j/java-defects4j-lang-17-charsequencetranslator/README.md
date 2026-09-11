# Sample provenance

- **Repository:** https://github.com/apache/commons-lang.git (`apache-commons-lang.git`)
- **Commit:** `2c1b5be146585dc8b27aedebca959f92da19094e`
- **File:** `src/main/java/org/apache/commons/lang3/text/translate/CharSequenceTranslator.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Lang-17** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`8a3e8603455c6dbfc595ab7a7e4a32f171a8b245`) and `after.java.test` the fixed revision
(`2c1b5be146585dc8b27aedebca959f92da19094e`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://issues.apache.org/jira/browse/LANG-720

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
