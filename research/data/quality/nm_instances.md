# Changes whose true correspondence is N:M

Provenance for Table 4 of `papers/introductory-paper`. **Authored, not generated** - this is a
hand-curated list found by reading annotator commentary, and it is deliberately not produced by a
script. A regex over prose comments emitting LaTeX would dress four hand-picked fixtures as a
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
why this list was found by reading `src/test/optimal_solutions/**/*.rs` rather than by enumeration.
It is a lower bound of unknown tightness. No denominator is quoted for it anywhere in the paper.

## How the list was found

```
grep -rniE "N:M|N-to-M|multi-to-multi|many.to.(one|many)|[0-9]+-to-[0-9]+|mutli" \
  src/test/optimal_solutions/*/*.rs
```

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
| `c-postgres-real-logic-change` | 2 statements to 1 | no |
| `scala-com-lihaoyi-mill-split-two-asserts-into-six-two-times` | string escapes, N:M | no |

Verbatim, from `src/test/optimal_solutions/`:

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

## Two facts the paper draws on, both re-checkable from the JSON

**The first two carry a `MultiMapGroup` sitting exactly at the N:M site**, which is what shows the
encoding is doing double duty - the same construct records genuine interchangeability in most
fixtures and a one-to-one approximation of an inexpressible correspondence in these:

```
c-pixaranimationstudios-opensubdiv-change-license-comment
  before: comment:8, comment:13, comment:15, comment:17, comment:23   (5 nodes)
  after:  comment:6                                                   (1 node)
scala-com-lihaoyi-mill-split-import-2
  before: import_declaration:1/namespace_selectors:1/identifier:2      (1 node)
  after:  import_declaration:2/identifier:3, import_declaration:2/identifier:4
```

Neither is a set of interchangeable candidates of which one survived; every member corresponds,
and the group's own semantics (realize `min(N, M)` pairs, delete or insert the remainder) assert
the opposite. The other two instances carry no group at all - the correspondence was noted in
prose and left unencoded.

**`scala-com-lihaoyi-mill-split-two-asserts-into-six-two-times` postdates the corpus snapshot**
that `human_mapping_analysis.csv` defines, so it is outside the fixture set every rate in the
paper's Section 5 is measured against. Because this list is an existence result and feeds no
denominator, it is listed anyway, with the fact stated in the paper.
