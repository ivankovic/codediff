# Sample provenance

- **Repository:** https://github.com/FasterXML/jackson-databind.git (`FasterXML-jackson-databind.git`)
- **Commit:** `308ed4ed89b32d119b5994e457597275c6a0af7d`
- **File:** `src/main/java/com/fasterxml/jackson/databind/deser/std/NullifyingDeserializer.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **JacksonDatabind-39** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`e741034db7191de9985a9b3dcd667bbdf262ce3a`) and `after.java.test` the fixed revision
(`308ed4ed89b32d119b5994e457597275c6a0af7d`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://github.com/FasterXML/jackson-databind/issues/1108

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
