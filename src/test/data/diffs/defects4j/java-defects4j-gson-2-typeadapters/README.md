# Sample provenance

- **Repository:** https://github.com/google/gson.git (`google-gson.git`)
- **Commit:** `fe101c10bc3597d8e715a31d94d2cc0cc54b660f`
- **File:** `gson/src/main/java/com/google/gson/internal/bind/TypeAdapters.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Gson-2** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`10714ef0427536b679e9677f8417807e4cce017d`) and `after.java.test` the fixed revision
(`fe101c10bc3597d8e715a31d94d2cc0cc54b660f`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://github.com/google/gson/pull/719

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
