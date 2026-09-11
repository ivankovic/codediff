# Sample provenance

- **Repository:** https://github.com/google/gson.git (`google-gson.git`)
- **Commit:** `0f66f4fac441f7d7d7bc4afc907454f3fe4c0faa`
- **File:** `gson/src/main/java/com/google/gson/internal/UnsafeAllocator.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Gson-8** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`6f6af8050799bec5321d2c06cd3230daadbb6535`) and `after.java.test` the fixed revision
(`0f66f4fac441f7d7d7bc4afc907454f3fe4c0faa`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://github.com/google/gson/issues/817

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
