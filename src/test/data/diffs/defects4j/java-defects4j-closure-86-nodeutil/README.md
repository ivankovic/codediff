# Sample provenance

- **Repository:** https://github.com/google/closure-compiler.git (`google-closure-compiler.git`)
- **Commit:** `0907b6618a60b2de23c8f7ec2217a37dc5e9a091`
- **File:** `src/com/google/javascript/jscomp/NodeUtil.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Closure-86** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`5cd9c1efe90dc7c1be33cd7f8c1dcbaa9225909e`) and `after.java.test` the fixed revision
(`0907b6618a60b2de23c8f7ec2217a37dc5e9a091`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://storage.googleapis.com/google-code-archive/v2/code.google.com/closure-compiler/issues/issue-303.json

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
