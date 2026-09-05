# Changes whose true correspondence is N:M

Provenance for Table 4 of `papers/introductory-paper`. **Authored, not generated** - this is a
hand-curated list found by reading annotator commentary, and it is deliberately not produced by a
script. A regex over prose comments emitting LaTeX would dress a handful of hand-picked fixtures as a
measurement artifact and would silently drift the day someone rewords a comment; a named fixture
plus its verbatim comment is checkable in a way a derived count is not.

## The distinction this list is on one side of

* **Multiple optimal solutions exist** - several one-to-one mappings are equally correct. Recorded
  in `human_mapping.json` as a `MultiMapGroup`, measured by `analysis/ambiguity_report.py`, and
  reported as a rate (RQ3's Table 3). An algorithm can still emit a correct answer.
* **No one-to-one optimal solution exists** - the true correspondence is N:M, several nodes on one
  side corresponding to one or several on the other, all at once. *This list.* Ordered tree edit
  distance maps each node to at most one node by definition, so this is outside the codomain of
  every algorithm in the family, and outside what `human_mapping.json` can express either.

Nothing in the corpus format marks the second. An annotator who meets one records the closest
one-to-one approximation and writes the real correspondence in the fixture's test case, which is
why this list was found by reading `src/test/fixtures/**/*.rs` rather than by enumeration.
It is a lower bound of unknown tightness. No denominator is quoted for it anywhere in the paper.

## How the list was found

```
grep -rniE "[0-9]+:[0-9]+ (map|match)|N:M|N-to-M|multi-to-multi|many.to.(one|many)|[0-9]+-to-[0-9]+|mutli" \
  src/test/fixtures/*/*.rs
```

**The `[0-9]+:[0-9]+` alternative is load-bearing and was missing from the first version of this
sweep** (2026-08-22), which is how two instances were missed until 2026-08-23: annotators write
both "a 2:1 mapping" and "a 2-to-1 mapping", and the original pattern only caught the hyphenated
form and the literal string "N:M". If you extend this list, widen the pattern first and re-read
every hit - the detector is the whole methodology here.

That sweep also matches fixtures whose comments merely mention a `MultiMapGroup` (the first
category), and one - `handmade/kotlin_refactor_function.rs`, "there's no single correct general
heuristic" - that is about conflicting human preference between two fixtures, not about N:M.
Those were excluded by reading each hit. Re-run the sweep after adding fixtures; it is the only
detector that exists.

## The instances

| Fixture | Correspondence | Group at the site? |
|---|---|---|
| `c-pixaranimationstudios-opensubdiv-change-license-comment` | 5 comment nodes to 1 | yes |
| `scala-com-lihaoyi-mill-split-import-2` | 1 identifier to 2 | yes |
| `swift-apple-swift-argument-parser-refactor-and-improve-tests` | 2 string literals to 1 | yes |
| `tsx-kong-insomnia-rewrite-if-using-ternary-twice` | 2 statements to 1 | yes |
| `c-postgres-real-logic-change` | 2 statements to 1 | no |
| `scala-com-lihaoyi-mill-split-two-asserts-into-six-two-times` | string escapes, N:M | no |

Verbatim, from `src/test/fixtures/`:

* `full/c_pixaranimationstudios_opensubdiv_change_license_comment.rs`:
  "Requires a N:M match for perfect solution"
* `full/scala_com_lihaoyi_mill_split_import_2.rs`:
  "The best solution would require a many-to-many map"
* `small/c_postgres_real_logic_change.rs`:
  "the ereport has a 2-to-1 mapping. Two separate instances of the same error got refactored into
  a single instance" ... "TODO: Deal with mutli-to-multi mapps. We can't represent this either in
  the mapping or visually at this time!"
* `full/scala_com_lihaoyi_mill_split_two_asserts_into_six_two_times.rs`:
  "The string escape sequences would probably need a N:M mapping"
* `full/swift_apple_swift_argument_parser_refactor_and_improve_tests.rs`:
  "True best mapping would be a 2:1 mapping of the string constant"
* `full/tsx_kong_insomnia_rewrite_if_using_ternary_twice.rs`:
  "Contains a 2:1 mapping not currently expressible with 1-1 maps"

## Two facts the paper draws on, both re-checkable from the JSON

**Four of the six carry a `MultiMapGroup` sitting exactly at the N:M site**, which is what shows the
encoding is doing double duty - the same construct records genuine interchangeability in most
fixtures and a one-to-one approximation of an inexpressible correspondence in these:

```
c-pixaranimationstudios-opensubdiv-change-license-comment
  before: comment:8, comment:13, comment:15, comment:17, comment:23   (5 nodes)
  after:  comment:6                                                   (1 node)
scala-com-lihaoyi-mill-split-import-2
  before: import_declaration:1/namespace_selectors:1/identifier:2      (1 node)
  after:  import_declaration:2/identifier:3, import_declaration:2/identifier:4
swift-apple-swift-argument-parser-refactor-and-improve-tests
  4 groups, each 2 -> 1 on the merged literal: line_string_literal, line_str_text, and its quotes
tsx-kong-insomnia-rewrite-if-using-ternary-twice
  16 groups, each 2 -> 1: expression_statement, assignment_expression, member_expression, ...
```

None of those is a set of interchangeable candidates of which one survived; every member
corresponds, and the group's own semantics (realize `min(N, M)` pairs, delete or insert the
remainder) assert the opposite. The remaining two instances carry no group at all - the correspondence was noted in
prose and left unencoded.

**Two instances postdate the corpus snapshot** that `human_mapping_analysis.csv` defines -
`scala-com-lihaoyi-mill-split-two-asserts-into-six-two-times` and
`swift-apple-swift-argument-parser-refactor-and-improve-tests` - so they are outside the fixture
set every rate in the paper's Section 5 is measured against. Because this list is an existence
result and feeds no denominator, they are listed anyway, with the fact stated in the paper.

## 2026-09-05: the sweep has drifted past this table - not yet triaged

Re-running the grep above now hits four fixtures the table does not list, none of which carries a
group at the site (`groups: 0` in all four):

* `full/python_aboutcode_org_license_expression_excellent_test_case.rs` -
  "True solution requires a N:M multi-map because two strings should map to one"
* `small/tsx_apache_superset_error_handling_change.rs` -
  "down with N:M support, not with a better matcher"
* `stratified/html_gohugoio_hugo_template_not_pure_html.rs` -
  "requires N:M mapping for the AST, but wouldn't if it parsed correctly"
* `stratified/html_twbs_bootstrap_not_html_template_extract_two_vars.rs`

Separately, and outside what any grep over `src/test/fixtures/` can reach: **`src/test/data/diffs.csv`'s
`comment` column is now a second, independent detector**, and it names three more -
`csharp-jellyfin-add-function` ("Requires N:M (2:2) mapping", `groups: 0`),
`rust-vercel-nextjs-refactoring-would-require-mulitmap-mapping` ("Requires N:M multi-map",
`groups: 3`) and `tsx-excalidraw-excalidraw-huge-file-with-real-logic-change` ("Requiers N:M
mapping", `groups: 15`). Those comments are written by the annotator in `human_solver` and are
where the unmarked residual of an N:M fixture gets explained, so they belong in the methodology
above; the stub grep alone is no longer "the only detector that exists".

Left as a note rather than seven new rows because the table is Table 4's provenance and each row is
a reading of the change itself, not of its comment. Whoever triages these should widen the "How the
list was found" section to cover the CSV before adding any of them.
