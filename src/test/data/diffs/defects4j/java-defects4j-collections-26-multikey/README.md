# Sample provenance

- **Repository:** https://github.com/apache/commons-collections.git (`apache-commons-collections.git`)
- **Commit:** `f8bd75d37ca12c5d49c1b628c33c0b45e2d082eb`
- **File:** `src/main/java/org/apache/commons/collections4/keyvalue/MultiKey.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Collections-26** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`3a9c4718ee0fd2eeef8b3ce151ee829fadbef5ae`) and `after.java.test` the fixed revision
(`f8bd75d37ca12c5d49c1b628c33c0b45e2d082eb`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://issues.apache.org/jira/browse/COLLECTIONS-576

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
