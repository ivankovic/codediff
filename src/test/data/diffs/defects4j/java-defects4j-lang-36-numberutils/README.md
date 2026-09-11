# Sample provenance

- **Repository:** https://github.com/apache/commons-lang.git (`apache-commons-lang.git`)
- **Commit:** `26bc3fe010d5154d3ccac526ec22c429fc3af499`
- **File:** `src/java/org/apache/commons/lang3/math/NumberUtils.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Lang-36** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`506bd018b3ca638cd0c9d1bdad627f6468a05bee`) and `after.java.test` the fixed revision
(`26bc3fe010d5154d3ccac526ec22c429fc3af499`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://issues.apache.org/jira/browse/LANG-521

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
