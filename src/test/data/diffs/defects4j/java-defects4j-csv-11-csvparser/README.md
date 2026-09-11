# Sample provenance

- **Repository:** https://github.com/apache/commons-csv.git (`apache-commons-csv.git`)
- **Commit:** `b67524da7fd146634c7112b23e95d1d45c398b82`
- **File:** `src/main/java/org/apache/commons/csv/CSVParser.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Csv-11** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`d3afa156e4ddf8ec76847504988e7979146d9fd6`) and `after.java.test` the fixed revision
(`b67524da7fd146634c7112b23e95d1d45c398b82`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://issues.apache.org/jira/browse/CSV-122

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
