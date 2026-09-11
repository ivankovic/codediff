# Sample provenance

- **Repository:** https://github.com/apache/commons-lang.git (`apache-commons-lang.git`)
- **Commit:** `1fe5439baf32af2114958e3cfc3512bd72c84773`
- **File:** `src/java/org/apache/commons/lang/text/StrBuilder.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Lang-47** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`2e8e3f46ba0a41d95c38875cf6a7153816262123`) and `after.java.test` the fixed revision
(`1fe5439baf32af2114958e3cfc3512bd72c84773`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://issues.apache.org/jira/browse/LANG-412

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
