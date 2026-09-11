# Sample provenance

- **Repository:** https://github.com/apache/commons-lang.git (`apache-commons-lang.git`)
- **Commit:** `8e2f4ddb9a1ecd7a1bf7d752c2c891d630287036`
- **File:** `src/main/java/org/apache/commons/lang3/math/NumberUtils.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Lang-24** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`7df70a8c6b14452767ac932a14640e32a1dc16da`) and `after.java.test` the fixed revision
(`8e2f4ddb9a1ecd7a1bf7d752c2c891d630287036`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://issues.apache.org/jira/browse/LANG-664

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
