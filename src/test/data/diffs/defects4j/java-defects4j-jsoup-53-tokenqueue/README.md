# Sample provenance

- **Repository:** https://github.com/jhy/jsoup.git (`jhy-jsoup.git`)
- **Commit:** `a229d7354da5210a728ce5d43158d5cd780772db`
- **File:** `src/main/java/org/jsoup/parser/TokenQueue.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Jsoup-53** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`c090381c55b6d275eebe60053d36f198ffe793ca`) and `after.java.test` the fixed revision
(`a229d7354da5210a728ce5d43158d5cd780772db`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://github.com/jhy/jsoup/issues/611

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
