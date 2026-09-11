# Sample provenance

- **Repository:** https://github.com/apache/commons-codec.git (`apache-commons-codec.git`)
- **Commit:** `d2f27093d7d95a07da901902f894d88b4ecc3e95`
- **File:** `src/main/java/org/apache/commons/codec/binary/StringUtils.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Codec-17** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`1a4d9cc2572d220664f1b7c377cd318cd253052e`) and `after.java.test` the fixed revision
(`d2f27093d7d95a07da901902f894d88b4ecc3e95`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://issues.apache.org/jira/browse/CODEC-229

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
