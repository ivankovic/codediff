# Sample provenance

- **Repository:** https://github.com/google/gson.git (`google-gson.git`)
- **Commit:** `2b08c88c09d14e0b1a68a982bab0bb18206df76b`
- **File:** `gson/src/main/java/com/google/gson/stream/JsonReader.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Gson-7** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`a02f575797d9295bff8fb92d266b0f9724e42098`) and `after.java.test` the fixed revision
(`2b08c88c09d14e0b1a68a982bab0bb18206df76b`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://github.com/google/gson/issues/604

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
