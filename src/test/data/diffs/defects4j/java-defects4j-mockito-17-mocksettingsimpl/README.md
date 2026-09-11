# Sample provenance

- **Repository:** https://github.com/mockito/mockito.git (`mockito-mockito.git`)
- **Commit:** `77cb2037314dd024eb53ffe2e9e06304088a2d53`
- **File:** `src/org/mockito/internal/creation/MockSettingsImpl.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Mockito-17** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`b6790ee34d8d858a89b80a9ae40f19d87ce1dab8`) and `after.java.test` the fixed revision
(`77cb2037314dd024eb53ffe2e9e06304088a2d53`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://code.google.com/archive/p/mockito/issues/152

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
