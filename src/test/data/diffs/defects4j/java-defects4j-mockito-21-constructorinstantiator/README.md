# Sample provenance

- **Repository:** https://github.com/mockito/mockito.git (`mockito-mockito.git`)
- **Commit:** `8912aa176ea8414c2fc57df0d9b030b918630e9f`
- **File:** `src/org/mockito/internal/creation/instance/ConstructorInstantiator.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Mockito-21** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`c03a47fb2d3d4df94ba608c26a0a7570798b3611`) and `after.java.test` the fixed revision
(`8912aa176ea8414c2fc57df0d9b030b918630e9f`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://github.com/mockito/mockito/issues/92

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
