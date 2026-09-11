# Sample provenance

- **Repository:** https://github.com/FasterXML/jackson-core.git (`FasterXML-jackson-core.git`)
- **Commit:** `b40ac81d4a81736e2b7536b14db4ad070b598d2e`
- **File:** `src/main/java/com/fasterxml/jackson/core/util/TextBuffer.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **JacksonCore-1** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`a9e5c9f99bcc16d734251f682758004a3ecc3a1b`) and `after.java.test` the fixed revision
(`b40ac81d4a81736e2b7536b14db4ad070b598d2e`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://github.com/FasterXML/jackson-core/issues/98

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
