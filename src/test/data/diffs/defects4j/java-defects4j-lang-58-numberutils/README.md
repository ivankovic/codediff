# Sample provenance

- **Repository:** https://github.com/apache/commons-lang.git (`apache-commons-lang.git`)
- **Commit:** `15360774099b2a7230e020751acdf6979b6e3f58`
- **File:** `src/java/org/apache/commons/lang/math/NumberUtils.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Lang-58** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`868dd284443b6f950a2f360b0422dbf09a599ae9`) and `after.java.test` the fixed revision
(`15360774099b2a7230e020751acdf6979b6e3f58`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://issues.apache.org/jira/browse/LANG-300

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
