# Sample provenance

- **Repository:** https://github.com/mockito/mockito.git (`mockito-mockito.git`)
- **Commit:** `5a03bf5d0c9aedac9cfbf074833167c1eca6439f`
- **File:** `src/org/mockito/internal/util/reflection/GenericMetadataSupport.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Mockito-8** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`9fb7d8b62814f959ceca6096d785b96c11bdfd0a`) and `after.java.test` the fixed revision
(`5a03bf5d0c9aedac9cfbf074833167c1eca6439f`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://github.com/mockito/mockito/issues/114

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
