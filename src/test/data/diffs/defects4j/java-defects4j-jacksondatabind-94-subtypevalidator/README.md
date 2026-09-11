# Sample provenance

- **Repository:** https://github.com/FasterXML/jackson-databind.git (`FasterXML-jackson-databind.git`)
- **Commit:** `6799f8f10cc78e9af6d443ed6982d00a13f2e7d2`
- **File:** `src/main/java/com/fasterxml/jackson/databind/jsontype/impl/SubTypeValidator.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **JacksonDatabind-94** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`939e332d951822f451d307ac4fc394af659aa0c6`) and `after.java.test` the fixed revision
(`6799f8f10cc78e9af6d443ed6982d00a13f2e7d2`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://github.com/FasterXML/jackson-databind/issues/1931

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
