# Sample provenance

- **Repository:** https://github.com/jhy/jsoup.git (`jhy-jsoup.git`)
- **Commit:** `c98349a7fb5598f0cbac88130520171bd6f253c1`
- **File:** `src/main/java/org/jsoup/safety/Whitelist.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Jsoup-19** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`d0418222830f42f4f0c770e406f71454ea50e56d`) and `after.java.test` the fixed revision
(`c98349a7fb5598f0cbac88130520171bd6f253c1`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://github.com/jhy/jsoup/issues/127

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
