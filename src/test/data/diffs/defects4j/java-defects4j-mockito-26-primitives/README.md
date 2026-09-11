# Sample provenance

- **Repository:** https://github.com/mockito/mockito.git (`mockito-mockito.git`)
- **Commit:** `af44738c7de74f24e37ea0c1242e73b07c3f4362`
- **File:** `src/org/mockito/internal/util/Primitives.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Mockito-26** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`41441c6b00e64c9f1f1275ff01a0ac4f7c4ae13e`) and `after.java.test` the fixed revision
(`af44738c7de74f24e37ea0c1242e73b07c3f4362`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://code.google.com/archive/p/mockito/issues/352

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
