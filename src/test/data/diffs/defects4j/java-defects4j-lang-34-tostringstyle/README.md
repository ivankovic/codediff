# Sample provenance

- **Repository:** https://github.com/apache/commons-lang.git (`apache-commons-lang.git`)
- **Commit:** `a2bb711aee6621b21cdd22e6d7e4bff2f7236235`
- **File:** `src/main/java/org/apache/commons/lang3/builder/ToStringStyle.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Lang-34** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`5c3ec55e15922c58bb2f39145de9fe641840bb50`) and `after.java.test` the fixed revision
(`a2bb711aee6621b21cdd22e6d7e4bff2f7236235`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://issues.apache.org/jira/browse/LANG-586

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
