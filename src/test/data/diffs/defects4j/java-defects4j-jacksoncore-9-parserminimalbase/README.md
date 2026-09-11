# Sample provenance

- **Repository:** https://github.com/FasterXML/jackson-core.git (`FasterXML-jackson-core.git`)
- **Commit:** `698f3a73d4e20594357e10fc7bb5db57bb8f7780`
- **File:** `src/main/java/com/fasterxml/jackson/core/base/ParserMinimalBase.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **JacksonCore-9** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`4b041e97686325afb2341ea009eb005e0b42d112`) and `after.java.test` the fixed revision
(`698f3a73d4e20594357e10fc7bb5db57bb8f7780`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://github.com/FasterXML/jackson-core/issues/188

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
