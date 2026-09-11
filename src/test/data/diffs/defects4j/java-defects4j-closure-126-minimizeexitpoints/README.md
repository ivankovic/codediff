# Sample provenance

- **Repository:** https://github.com/google/closure-compiler.git (`google-closure-compiler.git`)
- **Commit:** `bd2803b6d9ab600906b262ae51cb3591160b5f3c`
- **File:** `src/com/google/javascript/jscomp/MinimizeExitPoints.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Closure-126** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`211dc0161ae737f59cac22f30b048d56a059d14b`) and `after.java.test` the fixed revision
(`bd2803b6d9ab600906b262ae51cb3591160b5f3c`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://storage.googleapis.com/google-code-archive/v2/code.google.com/closure-compiler/issues/issue-936.json

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
