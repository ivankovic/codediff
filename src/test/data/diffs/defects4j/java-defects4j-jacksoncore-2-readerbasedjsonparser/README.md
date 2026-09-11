# Sample provenance

- **Repository:** https://github.com/FasterXML/jackson-core.git (`FasterXML-jackson-core.git`)
- **Commit:** `38d6e35d1f1a9b48193804925517500de8efee1f`
- **File:** `src/main/java/com/fasterxml/jackson/core/json/ReaderBasedJsonParser.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **JacksonCore-2** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`098ece8564ed5d37f483c3bfb45be897ed8974cd`) and `after.java.test` the fixed revision
(`38d6e35d1f1a9b48193804925517500de8efee1f`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://github.com/FasterXML/jackson-core/issues/105

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
