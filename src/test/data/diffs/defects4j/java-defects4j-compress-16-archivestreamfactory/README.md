# Sample provenance

- **Repository:** https://github.com/apache/commons-compress.git (`apache-commons-compress.git`)
- **Commit:** `9bdacbfb9631d3a3710a64f35482c643b78a2e79`
- **File:** `src/main/java/org/apache/commons/compress/archivers/ArchiveStreamFactory.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Compress-16** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`811fb4e1f7cb0b87d9af62cc892ac06a413eb560`) and `after.java.test` the fixed revision
(`9bdacbfb9631d3a3710a64f35482c643b78a2e79`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://issues.apache.org/jira/browse/COMPRESS-191

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
