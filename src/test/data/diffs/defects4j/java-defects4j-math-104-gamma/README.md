# Sample provenance

- **Repository:** https://github.com/apache/commons-math.git (`apache-commons-math.git`)
- **Commit:** `35414bc4f4ef03ef12e99c027398e5dc84682a9e`
- **File:** `src/java/org/apache/commons/math/special/Gamma.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Math-104** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`5bc120c5dff3cdc44f9168800a9d0a1fa538e44d`) and `after.java.test` the fixed revision
(`35414bc4f4ef03ef12e99c027398e5dc84682a9e`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://issues.apache.org/jira/browse/MATH-166

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
