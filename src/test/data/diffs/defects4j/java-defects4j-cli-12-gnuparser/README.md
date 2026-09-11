# Sample provenance

- **Repository:** https://github.com/apache/commons-cli.git (`apache-commons-cli.git`)
- **Commit:** `aa050a69b2395f496f953f4f9c4f63a475dcf430`
- **File:** `src/java/org/apache/commons/cli/GnuParser.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Cli-12** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`b8f825936796e2f5b6663de021aefa53fa728f67`) and `after.java.test` the fixed revision
(`aa050a69b2395f496f953f4f9c4f63a475dcf430`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://issues.apache.org/jira/browse/CLI-1

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
