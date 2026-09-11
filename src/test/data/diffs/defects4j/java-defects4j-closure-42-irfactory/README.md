# Sample provenance

- **Repository:** https://github.com/google/closure-compiler.git (`google-closure-compiler.git`)
- **Commit:** `fb01427a445ebfeb3af5e220a98ccaced339b73e`
- **File:** `src/com/google/javascript/jscomp/parsing/IRFactory.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Closure-42** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`f74939a64a205d89721383c24874fe4e41439cfd`) and `after.java.test` the fixed revision
(`fb01427a445ebfeb3af5e220a98ccaced339b73e`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://storage.googleapis.com/google-code-archive/v2/code.google.com/closure-compiler/issues/issue-644.json

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
