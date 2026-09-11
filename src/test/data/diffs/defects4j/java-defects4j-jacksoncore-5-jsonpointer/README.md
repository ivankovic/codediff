# Sample provenance

- **Repository:** https://github.com/FasterXML/jackson-core.git (`FasterXML-jackson-core.git`)
- **Commit:** `0d9cd9fa434c0070638332b7f2243af0277461eb`
- **File:** `src/main/java/com/fasterxml/jackson/core/JsonPointer.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **JacksonCore-5** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`bfdc2852923f671452c66ddf261c87e7e2e5b497`) and `after.java.test` the fixed revision
(`0d9cd9fa434c0070638332b7f2243af0277461eb`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://github.com/FasterXML/jackson-core/issues/173

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
