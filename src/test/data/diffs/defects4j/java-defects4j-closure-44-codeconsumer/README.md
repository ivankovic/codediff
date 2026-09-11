# Sample provenance

- **Repository:** https://github.com/google/closure-compiler.git (`google-closure-compiler.git`)
- **Commit:** `64bb2291f9a9bbab67d865dffe603f8a0df8ef30`
- **File:** `src/com/google/javascript/jscomp/CodeConsumer.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Closure-44** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`3e66c9c827308d2f549a16440e3f0ef7fd844e04`) and `after.java.test` the fixed revision
(`64bb2291f9a9bbab67d865dffe603f8a0df8ef30`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://storage.googleapis.com/google-code-archive/v2/code.google.com/closure-compiler/issues/issue-620.json

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
