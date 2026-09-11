# Sample provenance

- **Repository:** https://github.com/FasterXML/jackson-databind.git (`FasterXML-jackson-databind.git`)
- **Commit:** `fe25f7e14d92f5d4746549e7df22e9af35fdf54b`
- **File:** `src/main/java/com/fasterxml/jackson/databind/util/TokenBuffer.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **JacksonDatabind-31** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`3bd5de685c1f1116b71e6c51d713528834773fdf`) and `after.java.test` the fixed revision
(`fe25f7e14d92f5d4746549e7df22e9af35fdf54b`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://github.com/FasterXML/jackson-databind/issues/984

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
