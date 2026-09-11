# Sample provenance

- **Repository:** https://github.com/apache/commons-lang.git (`apache-commons-lang.git`)
- **Commit:** `673385b43d0d5783039242460ba1c12b3f1f4e92`
- **File:** `src/java/org/apache/commons/lang/math/Fraction.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Lang-49** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`dab1bdfc0fec1d45def7d4e4870dc207bd954686`) and `after.java.test` the fixed revision
(`673385b43d0d5783039242460ba1c12b3f1f4e92`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://issues.apache.org/jira/browse/LANG-380

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
