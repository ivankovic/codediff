# Sample provenance

- **Repository:** https://github.com/FasterXML/jackson-core.git (`FasterXML-jackson-core.git`)
- **Commit:** `fa64390b1bd5f1435daa9d2b17a58594cfb22817`
- **File:** `src/main/java/com/fasterxml/jackson/core/JsonGenerator.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **JacksonCore-20** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`1a5c3655e2b1646c2b68bb40c99f7a7e62fa958a`) and `after.java.test` the fixed revision
(`fa64390b1bd5f1435daa9d2b17a58594cfb22817`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://github.com/FasterXML/jackson-core/issues/318

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
