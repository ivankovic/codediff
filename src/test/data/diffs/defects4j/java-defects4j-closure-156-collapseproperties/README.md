# Sample provenance

- **Repository:** https://github.com/google/closure-compiler.git (`google-closure-compiler.git`)
- **Commit:** `f39bbdd39c9ec2136f0a58a677bd2fdae77075a2`
- **File:** `src/com/google/javascript/jscomp/CollapseProperties.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Closure-156** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`bae144ca994994d3ada90c9311286e9277f86288`) and `after.java.test` the fixed revision
(`f39bbdd39c9ec2136f0a58a677bd2fdae77075a2`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://storage.googleapis.com/google-code-archive/v2/code.google.com/closure-compiler/issues/issue-389.json

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
