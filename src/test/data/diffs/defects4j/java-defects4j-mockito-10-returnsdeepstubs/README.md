# Sample provenance

- **Repository:** https://github.com/mockito/mockito.git (`mockito-mockito.git`)
- **Commit:** `1beeae39ac9f79d6a0db285dec311b78025ac062`
- **File:** `src/org/mockito/internal/stubbing/defaultanswers/ReturnsDeepStubs.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Mockito-10** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`b00a6d252f87c886e5f8830bbdb6c1af2bd0ee9c`) and `after.java.test` the fixed revision
(`1beeae39ac9f79d6a0db285dec311b78025ac062`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://github.com/mockito/mockito/issues/99

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
