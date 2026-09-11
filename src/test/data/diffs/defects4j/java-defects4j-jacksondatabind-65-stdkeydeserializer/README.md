# Sample provenance

- **Repository:** https://github.com/FasterXML/jackson-databind.git (`FasterXML-jackson-databind.git`)
- **Commit:** `9a666887d85349b35e0d80885a7c7cb38029467d`
- **File:** `src/main/java/com/fasterxml/jackson/databind/deser/std/StdKeyDeserializer.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **JacksonDatabind-65** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`9257bd6b6a0227c400e1d008d5ad03d2244a6155`) and `after.java.test` the fixed revision
(`9a666887d85349b35e0d80885a7c7cb38029467d`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://github.com/FasterXML/jackson-databind/issues/1429

Defects4J has since deprecated this bug (in version 3.0.0: JVM11.flaky); the AST-diff oracle and the replication package predate that and still carry it.

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
