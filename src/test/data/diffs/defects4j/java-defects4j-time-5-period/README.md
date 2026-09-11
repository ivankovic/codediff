# Sample provenance

- **Repository:** https://github.com/JodaOrg/joda-time.git (`JodaOrg-joda-time.git`)
- **Commit:** `a6cb59ed2280ab0a32995fa8b5f1a7b0d47cb815`
- **File:** `src/main/java/org/joda/time/Period.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Time-5** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`a38b5e0c620a4a4dc310d35105e3e432c4e91fc3`) and `after.java.test` the fixed revision
(`a6cb59ed2280ab0a32995fa8b5f1a7b0d47cb815`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://github.com/JodaOrg/joda-time/issues/79

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
