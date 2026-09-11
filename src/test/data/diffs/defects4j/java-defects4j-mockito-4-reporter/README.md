# Sample provenance

- **Repository:** https://github.com/mockito/mockito.git (`mockito-mockito.git`)
- **Commit:** `42b391d4209024292b6101048389570e4ec36b2a`
- **File:** `src/org/mockito/exceptions/Reporter.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Mockito-4** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`c25cb6bff7eb4774efda345e8bf9eaad4c66a652`) and `after.java.test` the fixed revision
(`42b391d4209024292b6101048389570e4ec36b2a`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://github.com/mockito/mockito/issues/187

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
