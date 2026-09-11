# Sample provenance

- **Repository:** https://github.com/apache/commons-cli.git (`apache-commons-cli.git`)
- **Commit:** `538fb6889f0387a40b821cfb315685e929110736`
- **File:** `src/java/org/apache/commons/cli2/WriteableCommandLine.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Cli-13** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`e2ba946ea0e01c36c84f97f483e1d22cfd64b917`) and `after.java.test` the fixed revision
(`538fb6889f0387a40b821cfb315685e929110736`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://issues.apache.org/jira/browse/CLI-61

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
