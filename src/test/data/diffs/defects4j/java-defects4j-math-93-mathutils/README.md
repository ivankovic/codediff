# Sample provenance

- **Repository:** https://github.com/apache/commons-math.git (`apache-commons-math.git`)
- **Commit:** `e4886c0a39944ba9fb7a2175cb2d1026747910e1`
- **File:** `src/java/org/apache/commons/math/util/MathUtils.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Math-93** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`89db6968bc088e8ad3b74dda22ef08acdbf172b0`) and `after.java.test` the fixed revision
(`e4886c0a39944ba9fb7a2175cb2d1026747910e1`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://issues.apache.org/jira/browse/MATH-240

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
