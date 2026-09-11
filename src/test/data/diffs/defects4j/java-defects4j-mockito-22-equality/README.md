# Sample provenance

- **Repository:** https://github.com/mockito/mockito.git (`mockito-mockito.git`)
- **Commit:** `d0c872e4b0837aef1e1635bf5f15d33c3d8d9698`
- **File:** `src/org/mockito/internal/matchers/Equality.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Mockito-22** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`920ec4c3efe3133aa5009fcc9757a3cd07c5ac02`) and `after.java.test` the fixed revision
(`d0c872e4b0837aef1e1635bf5f15d33c3d8d9698`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://code.google.com/archive/p/mockito/issues/484

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
