# Sample provenance

- **Repository:** https://github.com/FasterXML/jackson-databind.git (`FasterXML-jackson-databind.git`)
- **Commit:** `354e1390bc397ef048ffd5d905b0709a3a2585ef`
- **File:** `src/main/java/com/fasterxml/jackson/databind/type/TypeFactory.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **JacksonDatabind-19** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`1a24cc1755d787a576c32157a38e441cfe839964`) and `after.java.test` the fixed revision
(`354e1390bc397ef048ffd5d905b0709a3a2585ef`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://github.com/FasterXML/jackson-databind/issues/810

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
