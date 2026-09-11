# Sample provenance

- **Repository:** https://github.com/google/closure-compiler.git (`google-closure-compiler.git`)
- **Commit:** `dff6e0114f19c20aeb63393d67aa0880ff5745b5`
- **File:** `src/com/google/javascript/jscomp/CodeGenerator.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Closure-145** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`b1ffe42efdaaf0266b4ba10e05de124a885cf979`) and `after.java.test` the fixed revision
(`dff6e0114f19c20aeb63393d67aa0880ff5745b5`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://storage.googleapis.com/google-code-archive/v2/code.google.com/closure-compiler/issues/issue-190.json

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
