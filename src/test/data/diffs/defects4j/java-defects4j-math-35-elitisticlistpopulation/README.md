# Sample provenance

- **Repository:** https://github.com/apache/commons-math.git (`apache-commons-math.git`)
- **Commit:** `b9ca51f09f38e5a4d73bcdafd09080c91670379c`
- **File:** `src/main/java/org/apache/commons/math3/genetics/ElitisticListPopulation.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Math-35** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`c016eb832b7ceedadcce85fb8fc7864611d7a0ef`) and `after.java.test` the fixed revision
(`b9ca51f09f38e5a4d73bcdafd09080c91670379c`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://issues.apache.org/jira/browse/MATH-776

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
