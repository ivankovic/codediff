# Sample provenance

- **Repository:** https://github.com/apache/commons-cli.git (`apache-commons-cli.git`)
- **Commit:** `2ddaae14dd23643a4b5f71f8274534b0b34556a4`
- **File:** `src/java/org/apache/commons/cli/HelpFormatter.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Cli-23** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`88cf9e4ab6e1b127de46fb2540f4d1fffac56442`) and `after.java.test` the fixed revision
(`2ddaae14dd23643a4b5f71f8274534b0b34556a4`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://issues.apache.org/jira/browse/CLI-162

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
