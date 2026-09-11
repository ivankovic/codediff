# Sample provenance

- **Repository:** https://github.com/apache/commons-csv.git (`apache-commons-csv.git`)
- **Commit:** `8b3de71fd99d0fa07cb6a3a35b583bbb170aab66`
- **File:** `src/main/java/org/apache/commons/csv/CSVFormat.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Csv-15** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`e76c4d809cf2b39a9d7fe831b63e60891fe47f55`) and `after.java.test` the fixed revision
(`8b3de71fd99d0fa07cb6a3a35b583bbb170aab66`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://issues.apache.org/jira/browse/CSV-219

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
