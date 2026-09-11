# Sample provenance

- **Repository:** https://github.com/apache/commons-math.git (`apache-commons-math.git`)
- **Commit:** `94cf29c4e3b6a7f37ad9d85ba572409691ed7f23`
- **File:** `src/java/org/apache/commons/math/complex/Complex.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Math-96** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`a48a97b37fa591427f95e158beb33031daf06196`) and `after.java.test` the fixed revision
(`94cf29c4e3b6a7f37ad9d85ba572409691ed7f23`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://issues.apache.org/jira/browse/MATH-221

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
