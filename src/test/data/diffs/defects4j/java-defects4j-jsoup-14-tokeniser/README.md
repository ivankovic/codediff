# Sample provenance

- **Repository:** https://github.com/jhy/jsoup.git (`jhy-jsoup.git`)
- **Commit:** `437d8eca9f51dd0ed7af5b828412ee96ac192f17`
- **File:** `src/main/java/org/jsoup/parser/Tokeniser.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Jsoup-14** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`35ba2dd27d69d1d09fed6f7a3c690f2bd121cb0f`) and `after.java.test` the fixed revision
(`437d8eca9f51dd0ed7af5b828412ee96ac192f17`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://github.com/jhy/jsoup/issues/82

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
