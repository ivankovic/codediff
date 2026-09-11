# Sample provenance

- **Repository:** https://github.com/FasterXML/jackson-databind.git (`FasterXML-jackson-databind.git`)
- **Commit:** `ce114737139538f72d2a7a994db41befc32362fe`
- **File:** `src/main/java/com/fasterxml/jackson/databind/ser/std/EnumSerializer.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **JacksonDatabind-75** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`ef17f26b4ad3a28979cd06b1ddaeda99080135b3`) and `after.java.test` the fixed revision
(`ce114737139538f72d2a7a994db41befc32362fe`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://github.com/FasterXML/jackson-databind/issues/1543

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
