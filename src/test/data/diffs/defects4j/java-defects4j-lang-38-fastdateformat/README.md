# Sample provenance

- **Repository:** https://github.com/apache/commons-lang.git (`apache-commons-lang.git`)
- **Commit:** `825481f019e4482e5ea74d5b0b5e5c438535cb68`
- **File:** `src/java/org/apache/commons/lang3/time/FastDateFormat.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Lang-38** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`91ac16e0b4a74b437d86c2dd7a47272105b4317c`) and `after.java.test` the fixed revision
(`825481f019e4482e5ea74d5b0b5e5c438535cb68`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://issues.apache.org/jira/browse/LANG-538

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
