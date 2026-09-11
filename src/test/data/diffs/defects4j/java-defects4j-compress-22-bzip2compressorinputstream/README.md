# Sample provenance

- **Repository:** https://github.com/apache/commons-compress.git (`apache-commons-compress.git`)
- **Commit:** `71e4eeadcfc5eb390eca1142fc1f6ee5b1b4d5c1`
- **File:** `src/main/java/org/apache/commons/compress/compressors/bzip2/BZip2CompressorInputStream.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Compress-22** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`ad634c0c24e883ec4ba509122df4679c2245e37b`) and `after.java.test` the fixed revision
(`71e4eeadcfc5eb390eca1142fc1f6ee5b1b4d5c1`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://issues.apache.org/jira/browse/COMPRESS-253

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
