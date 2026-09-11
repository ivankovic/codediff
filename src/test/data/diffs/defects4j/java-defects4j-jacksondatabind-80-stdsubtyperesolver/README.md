# Sample provenance

- **Repository:** https://github.com/FasterXML/jackson-databind.git (`FasterXML-jackson-databind.git`)
- **Commit:** `974ccddfd718a06f889da2dcd58ebb6bc22c8dca`
- **File:** `src/main/java/com/fasterxml/jackson/databind/jsontype/impl/StdSubtypeResolver.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **JacksonDatabind-80** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`8c20a7670f71958c9af6c8daffab227d2d7ba87a`) and `after.java.test` the fixed revision
(`974ccddfd718a06f889da2dcd58ebb6bc22c8dca`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://github.com/FasterXML/jackson-databind/issues/1616

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
