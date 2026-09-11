# Sample provenance

- **Repository:** https://github.com/apache/commons-cli.git (`apache-commons-cli.git`)
- **Commit:** `99aa05af2bfef3980ad8f94230cd077e8d30c5ea`
- **File:** `src/java/org/apache/commons/cli/PosixParser.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Cli-20** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`b1331806960001da95424315b6103d755107b519`) and `after.java.test` the fixed revision
(`99aa05af2bfef3980ad8f94230cd077e8d30c5ea`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://issues.apache.org/jira/browse/CLI-165

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
