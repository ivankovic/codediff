# Sample provenance

- **Repository:** https://github.com/FasterXML/jackson-databind.git (`FasterXML-jackson-databind.git`)
- **Commit:** `1d57c20dfd79c64837cf1ec357ab402f2158227e`
- **File:** `src/main/java/com/fasterxml/jackson/databind/deser/std/MapDeserializer.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **JacksonDatabind-12** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`36d9b59eb1b74442abeea856dca61ebe8bc7b3a5`) and `after.java.test` the fixed revision
(`1d57c20dfd79c64837cf1ec357ab402f2158227e`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://github.com/FasterXML/jackson-databind/issues/735

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
