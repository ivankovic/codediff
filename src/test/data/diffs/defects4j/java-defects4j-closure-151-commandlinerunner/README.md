# Sample provenance

- **Repository:** https://github.com/google/closure-compiler.git (`google-closure-compiler.git`)
- **Commit:** `fe58a5d1750bab2c5d1d0ef16102c1000e761bd8`
- **File:** `src/com/google/javascript/jscomp/CommandLineRunner.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Closure-151** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`d00fc0c64dba6762b649f9690cd52c68b849e118`) and `after.java.test` the fixed revision
(`fe58a5d1750bab2c5d1d0ef16102c1000e761bd8`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://storage.googleapis.com/google-code-archive/v2/code.google.com/closure-compiler/issues/issue-74.json

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
