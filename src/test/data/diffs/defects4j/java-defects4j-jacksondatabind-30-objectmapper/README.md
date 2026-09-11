# Sample provenance

- **Repository:** https://github.com/FasterXML/jackson-databind.git (`FasterXML-jackson-databind.git`)
- **Commit:** `ca6c3fc55eb74e21fe90e18da33723fb99b22f21`
- **File:** `src/main/java/com/fasterxml/jackson/databind/ObjectMapper.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **JacksonDatabind-30** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`16aa30572b41b7e643e9ee08def2169681f8e2c5`) and `after.java.test` the fixed revision
(`ca6c3fc55eb74e21fe90e18da33723fb99b22f21`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://github.com/FasterXML/jackson-databind/issues/965

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
