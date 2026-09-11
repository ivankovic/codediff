# Sample provenance

- **Repository:** https://github.com/apache/commons-math.git (`apache-commons-math.git`)
- **Commit:** `d277738885e78a9339f12ebf10c3c26eb538c1cf`
- **File:** `src/main/java/org/apache/commons/math/ode/nonstiff/EmbeddedRungeKuttaIntegrator.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **Math-39** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`ead76ad6aceb93de5bcef423ace01650f0558804`) and `after.java.test` the fixed revision
(`d277738885e78a9339f12ebf10c3c26eb538c1cf`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://issues.apache.org/jira/browse/MATH-727

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
