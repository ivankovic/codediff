# Sample provenance

- **Repository:** https://github.com/apache/commons-compress.git (`apache-commons-compress.git`)
- **Commit:** `a933173f486614073b68b6c0a6ef09a9c72c94fc`
- **File:** `src/main/java/org/apache/commons/compress/archivers/ArchiveStreamFactory.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Compress-11** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`00cc49e76b4cb5837f91bdcae2b3dff6a2809f2a`) and `after.java.test` the fixed revision
(`a933173f486614073b68b6c0a6ef09a9c72c94fc`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://issues.apache.org/jira/browse/COMPRESS-171

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
