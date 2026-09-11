# Sample provenance

- **Repository:** https://github.com/mockito/mockito.git (`mockito-mockito.git`)
- **Commit:** `c0222c2dbfbd2b053961a46e4c2a3973aec55a75`
- **File:** `src/org/mockito/internal/stubbing/answers/AnswersValidator.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Mockito-37** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`3b603ebf4bd3b416a2a00b7729233ae44ec75943`) and `after.java.test` the fixed revision
(`c0222c2dbfbd2b053961a46e4c2a3973aec55a75`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://code.google.com/archive/p/mockito/issues/140

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
