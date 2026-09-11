# Sample provenance

- **Repository:** https://github.com/JodaOrg/joda-time.git (`JodaOrg-joda-time.git`)
- **Commit:** `3ba9ba799b3261b7332a467a88be142c83b298fd`
- **File:** `src/main/java/org/joda/time/Partial.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Time-4** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`bcb044669b4d1f8d334861ccbd169924d6ef3b54`) and `after.java.test` the fixed revision
(`3ba9ba799b3261b7332a467a88be142c83b298fd`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://github.com/JodaOrg/joda-time/issues/88

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
