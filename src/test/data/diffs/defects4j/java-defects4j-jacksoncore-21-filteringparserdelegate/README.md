# Sample provenance

- **Repository:** https://github.com/FasterXML/jackson-core.git (`FasterXML-jackson-core.git`)
- **Commit:** `96faf2ecd985dfc838e2bf6f6ae4d9d4b310861b`
- **File:** `src/main/java/com/fasterxml/jackson/core/filter/FilteringParserDelegate.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **JacksonCore-21** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`8296172c76c43edfb9831eac7fc012e4a32806ad`) and `after.java.test` the fixed revision
(`96faf2ecd985dfc838e2bf6f6ae4d9d4b310861b`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://github.com/FasterXML/jackson-core/issues/330

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
