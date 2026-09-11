# Sample provenance

- **Repository:** https://github.com/apache/commons-math.git (`apache-commons-math.git`)
- **Commit:** `ec6dbad43a1f82f61ec6dbd43fd7766e92762e52`
- **File:** `src/main/java/org/apache/commons/math/dfp/Dfp.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Math-54** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`1df5ae529c2e21a9ba87ba8a2701994a4c6a3d75`) and `after.java.test` the fixed revision
(`ec6dbad43a1f82f61ec6dbd43fd7766e92762e52`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://issues.apache.org/jira/browse/MATH-567

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
