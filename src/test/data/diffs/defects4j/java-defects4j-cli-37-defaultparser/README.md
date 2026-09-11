# Sample provenance

- **Repository:** https://github.com/apache/commons-cli.git (`apache-commons-cli.git`)
- **Commit:** `1bf9e6c551b6a2e7d37291673a1ff77c338ce131`
- **File:** `src/main/java/org/apache/commons/cli/DefaultParser.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Cli-37** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`9e51962309fac2806f5fe0e4e3df94e0e187607a`) and `after.java.test` the fixed revision
(`1bf9e6c551b6a2e7d37291673a1ff77c338ce131`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://issues.apache.org/jira/browse/CLI-265

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
