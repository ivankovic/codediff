# Sample provenance

- **Repository:** https://github.com/apache/commons-compress.git (`apache-commons-compress.git`)
- **Commit:** `11840dfde044fec90b0cb4a715ce9d213acea3ca`
- **File:** `src/main/java/org/apache/commons/compress/utils/ArchiveUtils.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Compress-39** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`593339ab62ce5db71fd42501a9ddea9fe698b9ca`) and `after.java.test` the fixed revision
(`11840dfde044fec90b0cb4a715ce9d213acea3ca`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://issues.apache.org/jira/browse/COMPRESS-351

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
