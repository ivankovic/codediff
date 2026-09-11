# Sample provenance

- **Repository:** https://github.com/FasterXML/jackson-core.git (`FasterXML-jackson-core.git`)
- **Commit:** `911cca0254267decd90a4b6a9c0610549309a451`
- **File:** `src/main/java/com/fasterxml/jackson/core/json/UTF8StreamJsonParser.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **JacksonCore-3** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`7ee38785ecdd2f5a56a41302d4482675ce0d7e68`) and `after.java.test` the fixed revision
(`911cca0254267decd90a4b6a9c0610549309a451`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://github.com/FasterXML/jackson-core/issues/111

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
