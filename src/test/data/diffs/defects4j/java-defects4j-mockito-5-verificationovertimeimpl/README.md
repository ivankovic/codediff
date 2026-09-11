# Sample provenance

- **Repository:** https://github.com/mockito/mockito.git (`mockito-mockito.git`)
- **Commit:** `42a24dde02923185db3f79ae57e7819f7d70af55`
- **File:** `src/org/mockito/internal/verification/VerificationOverTimeImpl.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Mockito-5** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`fff76563d9e0ed412dc828c53cfdc7d142997a31`) and `after.java.test` the fixed revision
(`42a24dde02923185db3f79ae57e7819f7d70af55`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://github.com/mockito/mockito/issues/152

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
