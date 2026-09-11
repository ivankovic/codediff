# Sample provenance

- **Repository:** https://github.com/apache/commons-lang.git (`apache-commons-lang.git`)
- **Commit:** `54c0f6ba9e6555146d0dad51bcb5c4ec581e21da`
- **File:** `src/main/java/org/apache/commons/lang3/ArrayUtils.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Lang-35** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`a5dc7f1e68f735d0f989a10c521bbc0f6c1ef3ae`) and `after.java.test` the fixed revision
(`54c0f6ba9e6555146d0dad51bcb5c4ec581e21da`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://issues.apache.org/jira/browse/LANG-571

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
