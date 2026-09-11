# Sample provenance

- **Repository:** https://github.com/apache/commons-lang.git (`apache-commons-lang.git`)
- **Commit:** `8a1042959df80c06dbfa83896594caa8e20ff9d6`
- **File:** `src/main/java/org/apache/commons/lang3/math/NumberUtils.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Lang-3** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`64cfee77e333d0c31a0fde0abe6dac3d97b0f078`) and `after.java.test` the fixed revision
(`8a1042959df80c06dbfa83896594caa8e20ff9d6`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://issues.apache.org/jira/browse/LANG-693

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
