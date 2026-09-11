# Sample provenance

- **Repository:** https://github.com/apache/commons-lang.git (`apache-commons-lang.git`)
- **Commit:** `bef3eb6ed060d5ae14da6ce5a1909675b8cf5650`
- **File:** `src/main/java/org/apache/commons/lang3/text/translate/NumericEntityUnescaper.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Lang-28** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`c0041cafc2fda3fb437009d5417ba5ebeb32ad35`) and `after.java.test` the fixed revision
(`bef3eb6ed060d5ae14da6ce5a1909675b8cf5650`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://issues.apache.org/jira/browse/LANG-617

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
