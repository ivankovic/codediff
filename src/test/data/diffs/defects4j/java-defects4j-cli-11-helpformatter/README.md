# Sample provenance

- **Repository:** https://github.com/apache/commons-cli.git (`apache-commons-cli.git`)
- **Commit:** `d36adebd3547279b709960c902c3fb7b89a9a4ef`
- **File:** `src/java/org/apache/commons/cli/HelpFormatter.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Cli-11** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`33abc034037e3ef5d85d1b03010122617c386827`) and `after.java.test` the fixed revision
(`d36adebd3547279b709960c902c3fb7b89a9a4ef`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://issues.apache.org/jira/browse/CLI-1

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
