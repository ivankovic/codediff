# Matching across node kinds: what the ground truth actually contains, 2026-09-21

**What this is.** A census of every place a human matched two nodes that the grammar calls
different things, over all **1,180 fixtures** with a tree mapping. It answers the question that
comes before any rule about cross-kind matching: what shapes exist, and which of them are
defensible?

Reproduce:
```
cargo run --release --features test-fixtures --bin analyze_human_mappings -- --kind-mismatches
```
Row-level data: `kind_mismatches.csv` next to this file (one row per entry, both node texts
included - the kinds alone do not separate the cases).

## The population is tiny, and entirely one operation

**110 of 4,649,286 paired entries - 0.0024%** - across 45 of 1,180 fixtures. 42 distinct unordered
kind pairs, 10 of which occur in both directions.

All 110 are `MatchButNotIdentical`, and that is forced rather than observed: `Identical` and
`Update` both require equal kinds by definition, so `MatchButNotIdentical` is the only operation
able to carry such a pair. The population is therefore complete - there is nowhere else in the
schema for a cross-kind match to hide.

**Two things this number is not.** It is not a measure of how often cross-kind matching is
*needed*: the human solver raised a `y`/`n` modal on every cross-kind match until 2026-09-21, and
a painter who hits friction records delete+insert instead, invisibly. 110 is a floor. And it is
not a measure of how often codediff gets these wrong - nothing here reads codediff's output at
all. This is strictly the shape of the ground truth.

## The rule set

Three facts per pair decide it, and **none of them compares kind strings**: is each node a leaf
(no *named* children), do the two parents correspond in the mapping's resolved correspondence, and
is either node inside an `ERROR` region.

| rule | condition | verdict | n | % |
| --- | --- | --- | ---: | ---: |
| **R1** | leaf pair, parents correspond | **allow** | 67 | 61% |
| **R2a** | leaf pair, parents do not correspond, identical text | **allow** | 14 | 13% |
| **R2b** | leaf pair, parents do not correspond, text differs | **option** | 21 | 19% |
| **R3** | either node is a composite | **option** | 5 | 5% |
| **R4** | either node inside `ERROR` | kind is noise; decide on text | 3 | 3% |

**allow 81 (74%), option 26 (24%).**

### Why leaf-ness is the axis

A leaf is one lexeme in one slot. When the parents correspond, the slot identity comes from *the
parent match*, not from the kinds - so a cross-kind leaf pair asserts only "this position's token
changed", which is exactly what `Update` means. A composite pair asserts that two whole structures
correspond, which is a modelling claim a reader can reasonably refuse.

This also explains the anonymous-token population without a special case for it: those are leaves
whose kind happens to equal their text, so they land in R1 for the same reason `uint64` ->
`uint64_t` does.

### Why the parent anchor matters, and what replaces it

37 of the 110 are leaves whose parents do **not** correspond - a token matched *across* a
restructure. `(a != b)` becoming `a == b ? x : y` matches `!=` to `==` while their enclosing
expressions are delete/insert. With no parent anchoring the slot, the claim rests on the reader's
account of the edit, so the default cannot be "allow".

Text identity is what replaces the anchor: the same lexeme surviving a restructure (`pwd` ->
`pwd` as `identifier` -> `field_identifier`, `parseInt` -> `parseInt` as `identifier` ->
`property_identifier`) is the same element by any reading. 14 of the 37 qualify.

### R3, and why its children must not be offered separately

The five composites are the genuine construct substitutions: `while_statement` <->
`do_statement`, `self_closing_tag` <-> `start_tag`, `type_arguments` <-> `arguments`,
`field_initializer` <-> `shorthand_field_initializer`, `integer_value` <-> `float_value` (CSS
values carry their unit as a child, so they are composites).

15 further rows are anonymous tokens *inside* those constructs - `*` -> `&` under
`pointer_declarator` -> `reference_declarator`, `[` -> `(` under `type_arguments` ->
`arguments`, `while` -> `do` under `while_statement` -> `do_statement`. They must **inherit** the
parent's verdict. Offering them independently would let a reader accept `while` -> `do` while
rejecting `while_statement` -> `do_statement`.

## Two approaches that did not work

**Families derived from `node-types.json` supertypes.** The obvious way to define "same lexical
family" per grammar, and it fails in both directions. Too coarse: `while_statement` and
`do_statement` are both subtypes of `statement` in tree-sitter-c and tree-sitter-c-sharp, so
"shared supertype implies compatible" admits the clearest construct substitution in the corpus.
Too sparse: `identifier` <-> `field_identifier` and `hash_key_symbol` <-> `simple_symbol` share no
supertype at all, because a grammar only groups what its own parser needed grouped. Any family
list built this way is part-fitted and part-missing. The leaf predicate needs no vocabulary and so
cannot rot as grammars are upgraded.

**Detecting the level shift by delimiter containment.** `raw_string` -> `string_content` pairs a
container against its own content (`'^from langchain\.'` *with* quotes against the text
*without*). The tempting test - one text strictly contains the other, differing only by a few
delimiter characters - fires on 7 rows of which only 3 are level shifts: `>` -> `/>` and `%i[` ->
`[` are ordinary token changes that happen to be substrings. Four false positives out of seven is
not a detector. The three level shifts were found by reading the instances, and the fix is to
correct those entries, not to build a rule.

## What follows from this

1. **R1 wants a schema change, not a matcher change.** 43 of the 67 R1 rows are anonymous tokens,
   which are `Update`s the schema cannot express: `Update` is defined as "same kind, different
   text" and an anonymous node cannot change text without changing kind. Either relax `Update` for
   anonymous pairs, or classify by text rather than kind.
2. **The predicate needs nothing from the grammars.** Leaf-ness, parent correspondence and
   `ERROR` membership are all read off the tree. Grammar naming is inconsistent in ways we cannot
   fix - `)` carrying the text `]` in Ruby percent-literals, `number` anonymous in TypeScript
   while `type_identifier` is named, one lexical class split across `identifier`,
   `field_identifier` and `property_identifier` - and a predicate that never compares kind strings
   is immune to all of it.
3. **R3's children inherit; they are never an independent choice.**
4. **Three entries are wrong as recorded** - the two `raw_string` -> `string_content` pairs in
   `shellscript-langchain-ai-langchain-some-interesting-raw-string-to-string-content` and the
   `string_content` -> `regex` pair in `shellscript-scikit-learn-scikit-learn-string-to-regex`. A container matched
   against the counterpart container's child. These are the only entries in this census that
   should be edited rather than modelled.
