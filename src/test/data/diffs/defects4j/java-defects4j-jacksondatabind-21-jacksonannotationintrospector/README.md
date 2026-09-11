# Sample provenance

- **Repository:** https://github.com/FasterXML/jackson-databind.git (`FasterXML-jackson-databind.git`)
- **Commit:** `707db7a972e2d088647450f9a890c438fb735933`
- **File:** `src/main/java/com/fasterxml/jackson/databind/introspect/JacksonAnnotationIntrospector.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **JacksonDatabind-21** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`44dea1f292933192ea5287d9b3e14a7daaef3c0f`) and `after.java.test` the fixed revision
(`707db7a972e2d088647450f9a890c438fb735933`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://github.com/FasterXML/jackson-databind/issues/677

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
