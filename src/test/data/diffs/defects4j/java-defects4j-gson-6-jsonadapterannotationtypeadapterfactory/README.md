# Sample provenance

- **Repository:** https://github.com/google/gson.git (`google-gson.git`)
- **Commit:** `dea305503ad8827121e8212248c271f1f2f90048`
- **File:** `gson/src/main/java/com/google/gson/internal/bind/JsonAdapterAnnotationTypeAdapterFactory.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Gson-6** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`67bd3a2cf6f9c9c6e7f615969b1918f68e03932d`) and `after.java.test` the fixed revision
(`dea305503ad8827121e8212248c271f1f2f90048`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://github.com/google/gson/pull/800

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
