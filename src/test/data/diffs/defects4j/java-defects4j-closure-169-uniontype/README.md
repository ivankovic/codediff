# Sample provenance

- **Repository:** https://github.com/google/closure-compiler.git (`google-closure-compiler.git`)
- **Commit:** `ac239c7c53aa4d6c3105f600dec8af69da530883`
- **File:** `src/com/google/javascript/rhino/jstype/UnionType.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Closure-169** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`b4c6a605c0aee776bf195c8d71fe2aeebb47665a`) and `after.java.test` the fixed revision
(`ac239c7c53aa4d6c3105f600dec8af69da530883`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://storage.googleapis.com/google-code-archive/v2/code.google.com/closure-compiler/issues/issue-791.json

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
