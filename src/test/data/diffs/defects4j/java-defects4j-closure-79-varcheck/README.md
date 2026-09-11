# Sample provenance

- **Repository:** https://github.com/google/closure-compiler.git (`google-closure-compiler.git`)
- **Commit:** `5d397618f3c86d9c444a4c4c6441267b8a89a21d`
- **File:** `src/com/google/javascript/jscomp/VarCheck.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Closure-79** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`f3c23f757da302483c86414ec8b9c502f10fce00`) and `after.java.test` the fixed revision
(`5d397618f3c86d9c444a4c4c6441267b8a89a21d`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://storage.googleapis.com/google-code-archive/v2/code.google.com/closure-compiler/issues/issue-367.json

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
