# Sample provenance

- **Repository:** https://github.com/google/closure-compiler.git (`google-closure-compiler.git`)
- **Commit:** `b347ce3a71e23a4ab92dc0a6c07d43e2f497c953`
- **File:** `src/com/google/javascript/jscomp/Normalize.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Closure-139** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`6f2539acd3c37bd7f0d031b43364f75a5cd6d5f7`) and `after.java.test` the fixed revision
(`b347ce3a71e23a4ab92dc0a6c07d43e2f497c953`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://storage.googleapis.com/google-code-archive/v2/code.google.com/closure-compiler/issues/issue-33.json

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
