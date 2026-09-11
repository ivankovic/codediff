# Sample provenance

- **Repository:** https://github.com/jhy/jsoup.git (`jhy-jsoup.git`)
- **Commit:** `a810d2e3615da9a37ad74a7db2ca8bc6945ab9a8`
- **File:** `src/main/java/org/jsoup/helper/W3CDom.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Jsoup-84** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`bdf1df7eb3ca76cdcdaca38f7df5d941bbb1c664`) and `after.java.test` the fixed revision
(`a810d2e3615da9a37ad74a7db2ca8bc6945ab9a8`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://github.com/jhy/jsoup/issues/848

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
