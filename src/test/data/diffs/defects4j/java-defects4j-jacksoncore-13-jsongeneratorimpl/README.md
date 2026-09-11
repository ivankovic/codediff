# Sample provenance

- **Repository:** https://github.com/FasterXML/jackson-core.git (`FasterXML-jackson-core.git`)
- **Commit:** `d58d420f116e854bfd7b155cc1aed4e32939e1da`
- **File:** `src/main/java/com/fasterxml/jackson/core/json/JsonGeneratorImpl.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **JacksonCore-13** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`350bb8f1d2727defbf75d5de38df694857505688`) and `after.java.test` the fixed revision
(`d58d420f116e854bfd7b155cc1aed4e32939e1da`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://github.com/FasterXML/jackson-core/pull/246

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
