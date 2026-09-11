# Sample provenance

- **Repository:** https://github.com/FasterXML/jackson-core.git (`FasterXML-jackson-core.git`)
- **Commit:** `5cddffa133e7de41fa9efb5962cf3d0cff9b3e89`
- **File:** `src/main/java/com/fasterxml/jackson/core/json/JsonWriteContext.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **JacksonCore-7** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`b0ceb4eeb69f20f0e4242736ae3e19ed2b9c5da3`) and `after.java.test` the fixed revision
(`5cddffa133e7de41fa9efb5962cf3d0cff9b3e89`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://github.com/FasterXML/jackson-core/issues/177

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
