# Sample provenance

- **Repository:** https://github.com/google/closure-compiler.git (`google-closure-compiler.git`)
- **Commit:** `e609670bb56e0c7216c9476a5f6a5594e7f1acdb`
- **File:** `src/com/google/javascript/rhino/JSDocInfoBuilder.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Closure-106** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`6a36f04485599820ff86441accee002b4deec2eb`) and `after.java.test` the fixed revision
(`e609670bb56e0c7216c9476a5f6a5594e7f1acdb`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://storage.googleapis.com/google-code-archive/v2/code.google.com/closure-compiler/issues/issue-19.json

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
