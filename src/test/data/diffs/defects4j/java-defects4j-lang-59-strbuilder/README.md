# Sample provenance

- **Repository:** https://github.com/apache/commons-lang.git (`apache-commons-lang.git`)
- **Commit:** `9b0b19a9162ae8d217b06716962b982703ab4d1b`
- **File:** `src/java/org/apache/commons/lang/text/StrBuilder.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Lang-59** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`aaaa779108e445c028e91372a8c8709c7746cf72`) and `after.java.test` the fixed revision
(`9b0b19a9162ae8d217b06716962b982703ab4d1b`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://issues.apache.org/jira/browse/LANG-299

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
