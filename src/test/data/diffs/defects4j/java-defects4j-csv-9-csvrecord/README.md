# Sample provenance

- **Repository:** https://github.com/apache/commons-csv.git (`apache-commons-csv.git`)
- **Commit:** `2480dffa6d8343b189c6fb57b88b325c45f0b83f`
- **File:** `src/main/java/org/apache/commons/csv/CSVRecord.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Csv-9** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`0331565fbaf33fab24bfd588948e653475e795d9`) and `after.java.test` the fixed revision
(`2480dffa6d8343b189c6fb57b88b325c45f0b83f`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://issues.apache.org/jira/browse/CSV-118

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
