# Sample provenance

- **Repository:** https://github.com/FasterXML/jackson-core.git (`FasterXML-jackson-core.git`)
- **Commit:** `be4386724ff18232aff492cb9145288df86ea61c`
- **File:** `src/main/java/com/fasterxml/jackson/core/sym/ByteQuadsCanonicalizer.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **JacksonCore-11** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`7ca3d1cb6f1317863b5b04a1b0a7bac2aee5eefd`) and `after.java.test` the fixed revision
(`be4386724ff18232aff492cb9145288df86ea61c`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://github.com/FasterXML/jackson-core/issues/216

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
