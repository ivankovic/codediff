# Sample provenance

- **Repository:** https://github.com/apache/commons-csv.git (`apache-commons-csv.git`)
- **Commit:** `de1838ea067f3fbc4c7c21b9eeae077c739ecb73`
- **File:** `src/main/java/org/apache/commons/csv/ExtendedBufferedReader.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Csv-1** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`0833f45bffd40f44ba6f294d84e9bac8a9ba0a37`) and `after.java.test` the fixed revision
(`de1838ea067f3fbc4c7c21b9eeae077c739ecb73`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://issues.apache.org/jira/browse/CSV-75

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
