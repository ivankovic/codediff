# Sample provenance

- **Repository:** https://github.com/FasterXML/jackson-databind.git (`FasterXML-jackson-databind.git`)
- **Commit:** `817a45a07040297a5db45dd04e79b010146b7e0d`
- **File:** `src/main/java/com/fasterxml/jackson/databind/ObjectReader.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **JacksonDatabind-14** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`0ec81c07bc4be13e1382f7191981ed3f0bf9ef39`) and `after.java.test` the fixed revision
(`817a45a07040297a5db45dd04e79b010146b7e0d`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://github.com/FasterXML/jackson-databind/issues/744

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
