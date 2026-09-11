# Sample provenance

- **Repository:** https://github.com/google/closure-compiler.git (`google-closure-compiler.git`)
- **Commit:** `9959716b01fc5231ae68bc7a24454ce45d341606`
- **File:** `src/com/google/javascript/jscomp/InlineObjectLiterals.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Closure-53** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`4adf024b5eb87f6b760b40e9923ed1391bca4152`) and `after.java.test` the fixed revision
(`9959716b01fc5231ae68bc7a24454ce45d341606`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://storage.googleapis.com/google-code-archive/v2/code.google.com/closure-compiler/issues/issue-545.json

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
