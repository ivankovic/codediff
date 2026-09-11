# Sample provenance

- **Repository:** https://github.com/apache/commons-lang.git (`apache-commons-lang.git`)
- **Commit:** `65a6458eaa2e1f88278e4bb5753a68bd825e3a80`
- **File:** `src/main/java/org/apache/commons/lang3/time/FastDateParser.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Lang-9** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`ad72b9f2bf37bd61af18bde67c1622d90a5d8766`) and `after.java.test` the fixed revision
(`65a6458eaa2e1f88278e4bb5753a68bd825e3a80`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://issues.apache.org/jira/browse/LANG-832

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
