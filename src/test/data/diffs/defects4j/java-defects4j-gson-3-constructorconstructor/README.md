# Sample provenance

- **Repository:** https://github.com/google/gson.git (`google-gson.git`)
- **Commit:** `64107353a37e623ed1f8fecb4422c24212cf6fe1`
- **File:** `gson/src/main/java/com/google/gson/internal/ConstructorConstructor.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Gson-3** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`9e5f86d10b3b3ff4ba0dfe7ba0722c9e640fcc20`) and `after.java.test` the fixed revision
(`64107353a37e623ed1f8fecb4422c24212cf6fe1`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://github.com/google/gson/issues/624

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
