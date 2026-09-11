# Sample provenance

- **Repository:** https://github.com/jhy/jsoup.git (`jhy-jsoup.git`)
- **Commit:** `3a7f39c507ed2b6809dc3a8bcc0a28ad19aab2f3`
- **File:** `src/main/java/org/jsoup/parser/TreeBuilder.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Jsoup-44** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`ec608674d877d2e906cab77bb09ecb2494167f0e`) and `after.java.test` the fixed revision
(`3a7f39c507ed2b6809dc3a8bcc0a28ad19aab2f3`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://github.com/jhy/jsoup/issues/552

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
