# Sample provenance

- **Repository:** https://github.com/FasterXML/jackson-core.git (`FasterXML-jackson-core.git`)
- **Commit:** `6feec20c842586056aa5b19c402b26be08c88ca3`
- **File:** `src/main/java/com/fasterxml/jackson/core/filter/FilteringParserDelegate.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **JacksonCore-22** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`74564bfb7ca768067b5554dc7c4373f43b8be94b`) and `after.java.test` the fixed revision
(`6feec20c842586056aa5b19c402b26be08c88ca3`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://github.com/FasterXML/jackson-core/issues/208

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
