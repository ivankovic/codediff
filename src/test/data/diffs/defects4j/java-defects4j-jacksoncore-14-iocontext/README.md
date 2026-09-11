# Sample provenance

- **Repository:** https://github.com/FasterXML/jackson-core.git (`FasterXML-jackson-core.git`)
- **Commit:** `3c2c9610b6c76e2b348eb057d0a8268681d573a6`
- **File:** `src/main/java/com/fasterxml/jackson/core/io/IOContext.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **JacksonCore-14** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`383f63242c59628d7bcef15a73eb1c9d44d7fb56`) and `after.java.test` the fixed revision
(`3c2c9610b6c76e2b348eb057d0a8268681d573a6`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://github.com/FasterXML/jackson-core/issues/255

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
