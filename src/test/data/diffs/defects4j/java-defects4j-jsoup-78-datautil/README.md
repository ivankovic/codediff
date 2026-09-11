# Sample provenance

- **Repository:** https://github.com/jhy/jsoup.git (`jhy-jsoup.git`)
- **Commit:** `2c4e79b104c0ff32566ce247617d47c0b39cc2c7`
- **File:** `src/main/java/org/jsoup/helper/DataUtil.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Jsoup-78** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`71561e09e0f29ec5fd1bb918206f3d8e42876518`) and `after.java.test` the fixed revision
(`2c4e79b104c0ff32566ce247617d47c0b39cc2c7`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://github.com/jhy/jsoup/issues/980

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
