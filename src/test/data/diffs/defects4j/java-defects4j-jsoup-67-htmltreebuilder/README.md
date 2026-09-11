# Sample provenance

- **Repository:** https://github.com/jhy/jsoup.git (`jhy-jsoup.git`)
- **Commit:** `fb8b60b4d3d202c6fa708f60b8b4a5a53836af24`
- **File:** `src/main/java/org/jsoup/parser/HtmlTreeBuilder.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Jsoup-67** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`2412188026aa32491e769d6221c16a3bda6e897b`) and `after.java.test` the fixed revision
(`fb8b60b4d3d202c6fa708f60b8b4a5a53836af24`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://github.com/jhy/jsoup/issues/955

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
