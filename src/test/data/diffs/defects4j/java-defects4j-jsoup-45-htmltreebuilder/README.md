# Sample provenance

- **Repository:** https://github.com/jhy/jsoup.git (`jhy-jsoup.git`)
- **Commit:** `3b4f9dfa91b6f1852c35baf79c4a13eacc6112c3`
- **File:** `src/main/java/org/jsoup/parser/HtmlTreeBuilder.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Jsoup-45** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`5be8b081ce931865d46b49ed44e19d3eafde748d`) and `after.java.test` the fixed revision
(`3b4f9dfa91b6f1852c35baf79c4a13eacc6112c3`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://github.com/jhy/jsoup/issues/575

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
