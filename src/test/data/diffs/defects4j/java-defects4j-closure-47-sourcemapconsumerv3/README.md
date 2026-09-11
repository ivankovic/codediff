# Sample provenance

- **Repository:** https://github.com/google/closure-compiler.git (`google-closure-compiler.git`)
- **Commit:** `aedce35970673f20696d5721acba13e986cc764c`
- **File:** `src/com/google/debugging/sourcemap/SourceMapConsumerV3.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Closure-47** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`c5d7b308ecf260bf6ccf4b20ac256074fc42768f`) and `after.java.test` the fixed revision
(`aedce35970673f20696d5721acba13e986cc764c`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://storage.googleapis.com/google-code-archive/v2/code.google.com/closure-compiler/issues/issue-575.json

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
