# Painting, attributed: what is left after a perfect matcher, 2026-09-15

**What this is.** A census of every place codediff's *rendering* disagrees with a hand-painted
ground truth, over all **603 painted fixtures** (2001 diff cases, 1062 with a human tree mapping;
287 of the paintings are two-preset). It **attributes** each disagreement - separating what a
better node matcher could fix from what only a rendering rule can - and **counts by runs and
fixtures instead of bytes**.

Reproduce the census (its current output is `painting_attribution.csv`):
```
cargo test --release --lib --features test-fixtures painting_failure_census -- --ignored --nocapture
```

The whole-corpus byte aggregate, the rule experiments and the range dumps quoted below came from
one-off exploratory tests that are not in the tree; their numbers are as measured on 2026-09-15.

## Why bytes stopped being the metric

The whole-corpus byte aggregate reads **0.1052%** (35,503 of
33,750,624 bytes) against a `< 1%` goal. It clears the goal by a factor of ten and **steers
nothing**: the number fell because the painted corpus grew to 33.75M bytes, not because the
rendering got better. Handmade-only is still 0.8940%.

The distortion is not subtle. One fixture (`rust-turbopack-module-rule`) carries 7,293 of the
35,503 bytes. A spurious grey `}` carries one. The mistake a reader notices over and over is the
second kind, and no byte-weighted ranking will ever surface it. **This census counts runs (one
contiguous stretch of one verdict-pair disagreement) and fixtures-affected, and reports bytes
last.**

Of 604 measurable rows, **370 agree exactly** with their painting and **234 disagree**.

## Attribution: a perfect matcher would not fix this

`painting_failure_census` renders three things per side per preset:

* **real** - `diff_code`'s own mapping, rendered. What a reader sees.
* **ideal** - the fixture's *human tree mapping* pushed through the same `TextDiff` (via
  `as_ast_diff_for_mapping`). What the renderer would paint if node matching were perfect.
* **painted** - the human painting that preset is answerable to.

| preset | real vs painted | **ideal vs painted** | real vs ideal | corpus bytes |
|---|---|---|---|---|
| full | 21,058 | **18,541 (88%)** | 6,659 | 16,872,037 |
| minimal | 14,445 | **14,412 (99.8%)** | 7,793 | 16,872,037 |

**Read the middle column and nothing else.** Rendering the *correct* mapping still disagrees with
the painting on 18,541 bytes under `FULL` and 14,412 under `MINIMAL` - 88% and 99.8% of what a
reader actually sees. Under `MINIMAL`, a perfect node matcher would remove essentially none of the
painting disagreement.

The three columns are **not a partition** and do not sum. `real vs ideal` is "the two renderings
differ", not "the matcher owns this much": a matcher error that happens to land on what the
painting wanted appears there and not in `real vs painted`. The claim this table supports is the
one-way one - *this much survives a perfect matcher* - and that claim is enough.

## `FULL` is now the worse preset, and it is the product default

| | fixtures |
|---|---|
| `FULL` much worse than `MINIMAL` (> 1pp) | **61** |
| `MINIMAL` much worse than `FULL` | 19 |
| within 1pp of each other | 154 |

`RenderOptions::default()` is `FULL`, so this is what daily use looks like.

**This inverts the whole prior record.** Every earlier painting pass was about `MINIMAL`
over-painting, with `FULL` as the well-behaved side - `java-add-exception-handling` 60.786/6.399,
`rust-add-if` 59.420/1.087 (percent of bytes mismatched, `MINIMAL`/`FULL`, 2026-09-01); `paint_reindent_only_moves`,
`paint_displaced_moves` and `paint_resized_moves` all exist to give `MINIMAL` a quieter reading
than `FULL`'s. Those levers worked. The residue they left is `FULL`-shaped, and it has no fix
history - which also means none of the three reverted attempts at `MINIMAL`'s column-shift
`Move` (the `minimal move -> - code` row below, which is calibration: every human-painted
column-shift move it protects is a code span, never a lone bracket).

## The families, by how many fixtures they touch

Renderer's own errors (`ideal` vs `painted`), keyed on what the disagreeing text is. `-` is
"unpainted, i.e. unchanged and in place".

| preset | ours -> theirs | text | runs | **fixtures** | bytes |
|---|---|---|---|---|---|
| minimal | move -> - | code | 259 | 61 | 7085 |
| full | - -> update | code | 225 | 54 | 1742 |
| full | move -> - | code | 194 | 33 | 5486 |
| full | - -> move | code | 175 | 50 | 3972 |
| full | **move -> -** | **punctuation** | **150** | **41** | **690** |
| full | - -> move | whitespace | 120 | 36 | 364 |
| full | move -> delete | whitespace | 111 | 5 | 697 |
| minimal | - -> move | code | 110 | 24 | 1816 |
| full | **- -> insert** | **whitespace** | **101** | **63** | **300** |
| minimal | - -> update | code | 90 | 24 | 720 |
| minimal | update -> insert | code | 60 | 31 | 873 |
| full | - -> delete | whitespace | 58 | 31 | 154 |
| full | - -> move | punctuation | 48 | 18 | 80 |
| minimal | - -> insert | punctuation | 44 | 26 | 50 |
| minimal | - -> delete | punctuation | 32 | 17 | 34 |

Note the bytes column on the two bold rows: 690 bytes over 41 fixtures, 300 over 63. **These are
the two widest-spread families in the corpus and they are 1,000 bytes between them.** Both are
invisible in every byte-ranked report.

### A. Punctuation painted grey - 150 runs, 41 fixtures

The reported symptom: a closing bracket that is *matched* is painted `Move` (grey) where the
painting wants nothing. Confirmed, quantified, and the widest-spread `Move` family there is.

The census's geometry breakdown isolates the single-row case at **89 runs over 35 fixtures, 121
bytes** - 1.4 bytes an occurrence. Every one is `col-shift`; `col-same` is 4 runs in 2 fixtures
corpus-wide, so **`crossed_backwards` is not involved** and this is `column_shift_is_meaningful`
firing on a token that shifted because something else on its line grew. The human tree mapping
calls the node `Identical` in almost all of them, exactly as reported.

The tokens, by fixture, are the ordinary ones: `;` `,` `{` `}` `)` `:` `);` `):` `};` `::`. The
worklist runs to 46 preset/fixture pairs; most contribute 2 runs and 2 bytes, one token on each
side. `rust-turbopack-module-rule` (20), `java-defects4j-mockito-19-finalmockcandidatefilter` (16)
and `typescript-refactor-interface` (16) are the only concentrations.

A dump of the raw and filtered range lists confirms the range being painted is **the token itself**, not a span
coalesced across it by `RangeMatch::extends`: on
`java-defects4j-mockito-17-mocksettingsimpl` the raw and filtered range lists both hold
`";" (21:63-21:64) <-> ";" (23:19-23:20)`. A rule may therefore be written against the range's own
text.

Nearly all of it is `FULL`-only, and that is by construction: under `MINIMAL`
`structural_punctuation: false` already drops a range that is solely punctuation, and
`restore_paired_brackets` deliberately excludes `Move` from restoration (a recorded regression on
`javascript-refactor-arrow-func` when `Move` was included).

### B. Inserted and deleted whitespace nobody paints - 101 runs, 63 fixtures

The widest-spread family in the corpus. Under `FULL` the human paints a run of added alignment
whitespace `Insert` (`c-freeciv-add-parameter-to-function`, 29 spaces on a continuation line);
codediff paints nothing. The deletion mirror is 58 runs over 31 fixtures.

This is interior whitespace collapse being unpaintable, on sixty-three fixtures. The mechanism is
in `own_content_span`: `diff::text` derives every range from node spans, and inter-token gap text
belongs to no node, so there is nothing to hang a verdict on. `own_content_span` gives up entirely
on a node whose own content is split across more than one gap, which is every container with
content between several children.

**No `RenderOptions` field reaches this** - it is a change to which ranges get built, not to which
are painted. Largest open item; not attempted here.

### C. Unpainted updates - 225 runs, 54 fixtures (`FULL`)

`- -> update`: the human paints an identifier `Update` and codediff paints nothing there, having
already narrowed the change to the inserted prefix: rename granularity, a narrowed `Update`
against a whole-identifier one. The mirror, `update -> insert` (60 runs / 31 fixtures under `MINIMAL`), is the same seam
from the other side: where the narrowing leaves one side's middle empty, the edit *is* an
insertion and the painter calls it one.

The obvious lever is the wrong one - see R3 below.

### D. Which node moved - 175 runs, 50 fixtures (`FULL`), plus its relabellings

`- -> move`: the human paints the node that relocated, codediff paints the ones it went past (or
nothing on that side). With `insert -> move` (66/29) and `delete -> move` (62/26) this is the
decision `move_attribution.md` records: a mapping does not say which of a matched pair moved, and
the two tree walks answer it differently. `reconcile_moves`/`paint_resized_moves` address part of
it. The rule the corpus's painters follow, stated plainly, is **the minority is the mover**: of the
two faithful accounts, they paint the one that marks fewer nodes.

### E. Language concentration

Renderer's own errors are not uniform by language. By fixtures affected:
`rust` 28 (`FULL`) / 23 (`MINIMAL`), `java` 27/22, `c` 20/16, `cpp` 15/14, `javascript` 12/11,
`go` 12/7, `html` 10/8, `css` 9/9, `xml` 9/2, `tsx` 8/7.

Two shapes worth separating. `css` disagreements are large-percentage whole-file reformats
(reformat-only relocations painted `Move` where `MINIMAL` wants nothing) - 9 of 16 painted CSS fixtures. `xml` is the opposite: **11 of 11
painted XML fixtures disagree**, at 0.011%-0.03% each - six bytes here, eight there, on 100KB
files. A uniform tiny error across an entire language is exactly what a fixtures-affected count
finds and a byte rate buries.

## Rules, measured

The rule experiments scored a candidate rule against the same paintings by post-processing
the range list (or rebuilding under different options), **without changing the product**. A rule
that does not improve the corpus here never needs to be written.

| rule | preset | baseline | candidate | delta | better | worse |
|---|---|---|---|---|---|---|
| R1 drop single-row punctuation-only `Move` | full | 21058 | 21013 | −45 | 28 | **19** |
| R1 | minimal | 14445 | 14445 | 0 | 0 | 0 |
| R2 `MINIMAL` keeps *edited* punctuation | minimal | 14445 | 14434 | −11 | 28 | 11 |
| R3 `FULL` paints whole updated pairs | full | 21058 | 47873 | **+26815** | 43 | **146** |

**R3 is rejected outright.** Turning `whole_pair_updates` on for `FULL` more than doubles the
disagreement and makes 146 fixtures worse. It was the plausible reading of family C - "a preset
asking for more paint gets less" - and it is wrong: the corpus was painted narrow under *both*
presets, exactly as that field's own doc comment claims. Cheap to kill, and worth having killed.

**R1 as stated is not a win either.** The 19 regressions are named fixtures - `css-wordpress-*`
reformats, `rust-next-font-imports-generator`, `python-api-change`, `typescript-async-await` - and
the obvious story about them is that the punctuation belongs to a construct that genuinely
relocated, so the painting marks it, while in the 28 that improve only the line shifted underneath
a token that never moved. **That story is inferred from the fixture names, not read** - see "Verified
vs. read once".

If it holds, the corpus is consistent and a discriminator exists; R1 simply is not it. The first
candidate is already written down in this repo - `text_painting_findings.md` rule 2, **brackets
share fate**, 426 pairs with zero exceptions. Variants measured:

| variant | preset | delta | better | worse |
|---|---|---|---|---|
| R1 drop every single-row punctuation-only `Move` | full | −45 | 28 | 19 |
| R1b drop unless a bracket partner is painted at all | full | −26 | 14 | 4 |
| **R1c drop unless a bracket partner is painted `Move`** | full | **−69** | **30** | **13** |
| R1d drop unless something else on its row is `Move` | full | −45 | 23 | 13 |

Every variant is exactly **zero** under `MINIMAL`, as R1 is: `structural_punctuation: false`
already drops these ranges there.

**R1c is the best predicate found.** `restore_paired_brackets`' own `covered` test asks whether
*some* range covers the partner, which keeps a `)` whose `(` sits inside an `Update` because the
line was rewritten around it - a different fate, not a shared one. Requiring the partner to be
painted `Move` too is the literal reading of "brackets share fate", and it beats the blanket rule
on both axes at once. Its 13 remaining regressions are dominated by whole-file relocations
(`css-wordpress-wordpress-go-to-one-line`, `-re-format-in-one-line`) and
`rust-next-font-imports-generator`, where everything really did move.

**The `ASTMappingReason` cross-tab does not separate them - and cannot, as collected.** The
reasons behind the punctuation family are dominated by `APTED("qualified_name")` (about 20 of the
30 fixtures R1c improves), with `IdenticalHashOfAncestor`, `IdenticalHash`, `MovedSubtree`,
`BottomUpPropagation`, `APTED("fast_fallback")`, `APTED("import_path_similarity")` and
`APTED("greedy_anchor_block")` making up the rest. But `typescript-add-type-annotations` is
`APTED("qualified_name")` too and R1c makes it *worse*, so the reason is not a separator on its
face.

The deeper reason it cannot be one from this data: **most of R1c's 13 regressions never appear in
this census at all.** They regress because R1c drops a punctuation `Move` the painting *agreed*
with - an agreement, so it is not a disagreement run and nothing here records its reason. A
reason-based predicate would have to be measured over the agreeing population as well, which this
census does not collect. Filed as the next measurement, not as a rejected idea.

**Where these lone punctuation ranges come from.** The range dump on
`c-cpython-autogenerated-code` shows the raw range list holding *no* punctuation-only `Move` at
all, and the filtered list holding two: `"{"` and `");"`. They are the residue of larger `Move`
ranges after `ranges_for_options`' unconditional trailing-whitespace trim - what a reader is left
with is a lone bracket because everything else in the range was whitespace. The same dump explains
the census's `text-differs` geometry label: the source is `"{"` and the destination is `"{\n"`,
a destination range normalised to end at column 0 of the next row. Not coalescing, and not a
multi-byte-character artifact.

**R2 is a small clean positive** and states a rule worth having independently of its size:
`MINIMAL` drops punctuation that merely moved or stayed, and keeps punctuation that was *inserted
or deleted*. An inserted comma is not noise - it is the edit. Today `structural_punctuation: false`
drops it regardless of operation, and `restore_paired_brackets` only brings back `Insert`/`Delete`
punctuation that happens to have a surviving partner. The families it targets are `minimal - ->
insert punctuation` (44 runs / 26 fixtures) and `minimal - -> delete punctuation` (32 / 17).

## Verified vs. read once

Verified directly: the punctuation range is the token and not a coalesced span (`probe`, two
fixtures); `text-unknown` geometry is 3 runs corpus-wide, so `text-differs` is real and not a
multi-byte-character artifact; R3's rejection and R1/R1b/R2's deltas are whole-corpus measurements,
not samples.

**Three places a reader could over-read these numbers.**

1. **The attribution table is not a partition** - see above. Only the middle column is a claim.
2. **The `ideal` column over-attributes `Move` to the renderer.** `as_ast_diff_for_mapping` leaves
   `ASTMappingReason::default()` on every entry, so the two reason-tagged escapes from `Move`
   (`known_pure_reindent` via `NestedConditionCollapse`/`WrapGrowth`, `known_pure_relocation` via
   `HeritageClauseGrowth`) cannot fire there. It never under-attributes.
3. **N:M fixtures render a pairing no human claimed.** `as_ast_diff_for_mapping` goes through
   `representative_entries`, which flattens each multi-map group by picking one arbitrary pairing.
   **14 painted fixtures carry groups** - `rust-algorithm-change`, `rust-error-handling`,
   `rust-firefox-webrenderer-borders`, `java-add-exception-handling`, `javascript-fix-promises`,
   `c-linux-small-bugfix`, `rust-add-to-existing-use`, `cpp-libreoffice-remove-two-wrapping-functions`,
   and six others. A fixture hot in the `ideal` ranking is not a renderer bug until it is checked
   against that list.

Read once, from the census tables: every family assignment above; the language rollup's split
between "reformat" and "uniform tiny error"; and the causal story about R1's 19 regressions - that
the punctuation there belongs to a construct that genuinely relocated - which is an inference from
the fixture names, not something read out of those fixtures' paintings.
