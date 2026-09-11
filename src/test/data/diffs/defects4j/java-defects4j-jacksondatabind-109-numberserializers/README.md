# Sample provenance

- **Repository:** https://github.com/FasterXML/jackson-databind.git (`FasterXML-jackson-databind.git`)
- **Commit:** `e287a62cd32832d5a9611d5b8f3bc06ec1310dc0`
- **File:** `src/main/java/com/fasterxml/jackson/databind/ser/std/NumberSerializers.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **JacksonDatabind-109** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`a475c0d526ba9b8343e28ad9543a46005b0842b3`) and `after.java.test` the fixed revision
(`e287a62cd32832d5a9611d5b8f3bc06ec1310dc0`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://github.com/FasterXML/jackson-databind/issues/2230

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
