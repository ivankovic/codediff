# Sample provenance

- **Repository:** https://github.com/google/closure-compiler.git (`google-closure-compiler.git`)
- **Commit:** `43c245f0ff8d409e81e25687e69d34666b7cf26a`
- **File:** `src/com/google/javascript/jscomp/CommandLineRunner.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Closure-83** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`840ddca5b28cea7563a5be20d2624478af67bc02`) and `after.java.test` the fixed revision
(`43c245f0ff8d409e81e25687e69d34666b7cf26a`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://storage.googleapis.com/google-code-archive/v2/code.google.com/closure-compiler/issues/issue-319.json

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
