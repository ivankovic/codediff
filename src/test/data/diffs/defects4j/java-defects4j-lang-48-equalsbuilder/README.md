# Sample provenance

- **Repository:** https://github.com/apache/commons-lang.git (`apache-commons-lang.git`)
- **Commit:** `e199d381f8c199801fee2d40a7f3ea1380700631`
- **File:** `src/java/org/apache/commons/lang/builder/EqualsBuilder.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Lang-48** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`97f1c120c092916f2f95439b6440a8977c66ee0a`) and `after.java.test` the fixed revision
(`e199d381f8c199801fee2d40a7f3ea1380700631`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://issues.apache.org/jira/browse/LANG-393

Defects4J has since deprecated this bug (in version 3.0.0: JVM11.Not.Repoducible); the AST-diff oracle and the replication package predate that and still carry it.

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
