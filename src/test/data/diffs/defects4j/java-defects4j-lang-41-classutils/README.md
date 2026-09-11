# Sample provenance

- **Repository:** https://github.com/apache/commons-lang.git (`apache-commons-lang.git`)
- **Commit:** `2dc56182579703153e5c9886b3ecd22bbc348b49`
- **File:** `src/java/org/apache/commons/lang/ClassUtils.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Lang-41** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`bb8709f3e30e7c13530dfef458a4c370783de2be`) and `after.java.test` the fixed revision
(`2dc56182579703153e5c9886b3ecd22bbc348b49`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://issues.apache.org/jira/browse/LANG-535

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
