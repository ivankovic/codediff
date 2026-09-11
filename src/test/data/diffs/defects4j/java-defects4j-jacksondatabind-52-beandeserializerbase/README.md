# Sample provenance

- **Repository:** https://github.com/FasterXML/jackson-databind.git (`FasterXML-jackson-databind.git`)
- **Commit:** `ab8a20f6447da482bd0430498b482d06867c9c50`
- **File:** `src/main/java/com/fasterxml/jackson/databind/deser/BeanDeserializerBase.java`
- **Research dataset:** defects4j

This fixture is Defects4J bug **JacksonDatabind-52** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`2202ca51cf3e61765c9ac96326633deb2ff37318`) and `after.java.test` the fixed revision
(`ab8a20f6447da482bd0430498b482d06867c9c50`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: https://github.com/FasterXML/jackson-databind/issues/999

This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
