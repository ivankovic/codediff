# Sample provenance

- **Repository:** https://github.com/FasterXML/jackson-databind.git (`FasterXML-jackson-databind.git`)
- **Commit:** `466f706901d39ae95393c6c7b1d0b544247922f1`
- **File:** `src/main/java/com/fasterxml/jackson/databind/ser/AnyGetterWriter.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **JacksonDatabind-10** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`6262dd76625e16d10ee45bc90ae76b02101cd65a`) and `after.java.test` the fixed revision
(`466f706901d39ae95393c6c7b1d0b544247922f1`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://github.com/FasterXML/jackson-databind/issues/705

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
