# Sample provenance

- **Repository:** https://github.com/google/closure-compiler.git (`google-closure-compiler.git`)
- **Commit:** `49e9565febba904484396e2aef7dbe86f55e9cc5`
- **File:** `src/com/google/javascript/jscomp/CommandLineRunner.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Closure-107** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`aae547f4d911dfbf42ca5a31aeb401ac83725a14`) and `after.java.test` the fixed revision
(`49e9565febba904484396e2aef7dbe86f55e9cc5`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://storage.googleapis.com/google-code-archive/v2/code.google.com/closure-compiler/issues/issue-1135.json

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
