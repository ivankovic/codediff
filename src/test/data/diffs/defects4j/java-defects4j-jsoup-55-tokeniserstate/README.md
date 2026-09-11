# Sample provenance

- **Repository:** https://github.com/jhy/jsoup.git (`jhy-jsoup.git`)
- **Commit:** `f0f0e41e6c581de43dfaa98f5c2af52e43e42e62`
- **File:** `src/main/java/org/jsoup/parser/TokeniserState.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Jsoup-55** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`aa81e10c34f48a3c4ac7160aa90ee18af4f5c0c2`) and `after.java.test` the fixed revision
(`f0f0e41e6c581de43dfaa98f5c2af52e43e42e62`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://github.com/jhy/jsoup/issues/746

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
