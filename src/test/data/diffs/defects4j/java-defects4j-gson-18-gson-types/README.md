# Sample provenance

- **Repository:** https://github.com/google/gson.git (`google-gson.git`)
- **Commit:** `b1fb9ca9a1bea5440bc6a5b506ccf67236b08243`
- **File:** `gson/src/main/java/com/google/gson/internal/$Gson$Types.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Gson-18** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`08bbb226f11a1f7f76835f953e700d905e1fab4d`) and `after.java.test` the fixed revision
(`b1fb9ca9a1bea5440bc6a5b506ccf67236b08243`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://github.com/google/gson/issues/1107

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
