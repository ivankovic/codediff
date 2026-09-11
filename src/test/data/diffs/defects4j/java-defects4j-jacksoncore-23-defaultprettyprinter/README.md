# Sample provenance

- **Repository:** https://github.com/FasterXML/jackson-core.git (`FasterXML-jackson-core.git`)
- **Commit:** `157f2c490003c7494af8ecb7d57880cda1fde736`
- **File:** `src/main/java/com/fasterxml/jackson/core/util/DefaultPrettyPrinter.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **JacksonCore-23** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`dc5c82e4af87577ac1b540b3f7e9279159185278`) and `after.java.test` the fixed revision
(`157f2c490003c7494af8ecb7d57880cda1fde736`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://github.com/FasterXML/jackson-core/issues/502

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
