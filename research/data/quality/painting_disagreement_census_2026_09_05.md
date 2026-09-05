# Rendering vs. painting: the 86 fixtures that were never measured, 2026-09-05

**What this is.** Every painting stub outside `handmade` carried the generated placeholder
`assert_matches_human_painting_within_limit(name, 100.0)` — "Not measured yet: 100.0 passes
unconditionally". 86 of them, 84 of those `stratified`, i.e. **84 of the corpus's 149 painted
fixtures had a painting test that could not fail**. This measures all 86, clamps them at their real
rate, and reads the disagreements against the family taxonomy in
`painting_disagreement_census_2026_09_01.md`.

Companion to that file, not a replacement: the patterns it names still hold, and three of them turn
up here. What is new is that this is the first time the *sampled* corpus (real commits, in CSS,
HTML, C, Go, Scala, Lua) has been read at all — the 2026-09-01 census covered 53 fixtures that were
almost entirely `handmade`.

Reproduce:
```
FIXTURES="<86 names>" cargo test --release --lib --features test-fixtures \
  measure_stub_fixtures -- --ignored --nocapture
FIXTURES="<name>,..." cargo test --release --lib --features test-fixtures \
  painting_disagreement_detail_batch -- --ignored --nocapture
```

## The headline: most of them already agree exactly

| | count |
|---|---|
| measured | 86 |
| **exact (0.000% under both presets)** | **59** |
| disagree at all | 27 |

Aggregate over all 86, pooled bytes: **minimal 3.767%, full 4.398%** (1993 / 2327 of 52,913 bytes).
Note this pooled figure is over *these 86 small files only*. Byte-weighted over the whole painted
corpus the picture is different, because handmade holds 1.53M of the 2.37M painted bytes — see
"Resolved" at the bottom.

The 27 that disagree concentrate by language in a way the handmade corpus could not have shown:
**C 8, CSS 5, HTML 5**, Rust 3, C++ 2, and one each of Scala, Go, JavaScript, Lua. CSS and HTML
between them are 10 of 27 and hold the four worst rates in the set.

## Families

Three are the 2026-09-01 census's patterns showing up in new languages. Two are new, and both are
new because the handmade corpus has no fixture of the shape.

### A. Reformat-only relocation: `Move` where Minimal wants nothing (pattern 2, off Rust)

The 2026-09-01 census recorded that `paint_reindent_only_moves`' `known_pure_reindent` check only
recognises Rust's `if let`-chain collapse, so relocation in any other construct gets painted `Move`
under both presets. Every CSS reformat fixture here is that, at whole-file scale:

| Fixture | Minimal % | Full % |
|---|---|---|
| css-wordpress-wordpress-re-format-in-one-line | 84.146 | 4.065 |
| css-wordpress-wordpress-reformat-and-fix-lint-errors | 64.394 | 50.758 |
| css-wordpress-wordpress-go-to-one-line | 30.833 | 3.333 |

Signature in the dump: long runs of *unchanged* declaration text with `ours=Some(Move) theirs=None`,
and a Full rate an order of magnitude below the Minimal one — the same 60/6, 59/1, 35/3 shape
`java-add-exception-handling`, `rust-add-if` and `typescript-add-error-handling` already showed.
A CSS minifier run (`border:1px solid #ccc;\n  border-radius:4px;…` → one line) relocates every
declaration; codediff has only the Full-shaped answer.

**Not a new lever.** Three past sessions tried and reverted Move-heuristic fixes in this family.

### B. Files that are not the language they are named (HTML holding Go templates / YAML)

| Fixture | Minimal % | Full % |
|---|---|---|
| html-gohugoio-hugo-template-not-pure-html-2 | 77.670 | 77.670 |
| html-prettier-prettier-not-pure-html-includes-yaml-as-well | 39.227 | 41.436 |
| html-twbs-bootstrap-not-html-template-extract-two-vars | 24.734 | 24.127 |
| html-gohugoio-hugo-template-not-pure-html | 16.798 | 22.572 |

Whole `{{ … }}` directives painted `Insert`/`Move` by the human and `Update`/nothing by codediff, or
the reverse. These fixtures' own stub comments already say the change "requires N:M mapping for the
AST, but wouldn't if it parsed correctly". **This is a parse-quality gap, not a renderer gap** — no
`RenderOptions` field reaches it, and nothing in the painting machinery should be changed on their
account. They are 4 of the 5 worst rates in the set and they inflate any aggregate that includes
them — and note the report's "excluding parse errors" bucket does **not** exclude them, because
three of the four parse without error. See "Resolved" below.

### C. Move attribution: which side is the mover (already decided, see `move_attribution.md`)

`c-neovim-neovim-small-change` (48.052 / 59.091) is the cleanest instance in the corpus. An
`#include` and two `extern` declarations swap order. The human paints the `#include` as the thing
that moved; codediff paints the two `extern` lines instead. Both are faithful to the same mapping —
this is exactly the decision `move_attribution.md` records — and the fixture is small enough
(308 bytes) that one such disagreement is 59% of it.

Also here: `c-genymobile-scrcpy-add-to-import-path-and-move-imports-around` (4.314 / 5.392),
`css-wordpress-wordpress-remove-one-rule` (11.391 both), `go-lazygit-switch-to-strings`
(2.564 / 3.812).

### D. Rename granularity: narrowed `Update` vs whole-identifier `Update` (pattern 5)

`c-genymobile-scrcpy-rename-defines` (2.521 / **38.992**) is the sharpest case and the sharpest
*preset split* in the set. `EVENT_NEW_FRAME` → `SC_EVENT_NEW_FRAME`, five times:

* Minimal — codediff paints the added `SC_` as `Update`, the human as `Insert`. 3 bytes × 5.
* Full — the human paints the **whole identifier** `Update` on both sides; codediff paints nothing
  there at all (`ours=None`), having already narrowed the change to the inserted prefix.

So a preset asking for *more* paint gets *less*, because narrowing happens before the preset is
consulted. Same shape at smaller scale in `c-genymobile-scrcpy-rename-and-add-a-define`
(0.595 / 3.274), `rust-rust-lang-rust-change-use` (7.092 / 7.008),
`rust-rust-lang-rust-update-comment` (0.064 / 4.499), and both
`cpp-ollama-ollama-update-commit-hash-*` (0.592).

### E. NEW — interior whitespace collapse is unpaintable

`c-openssl-openssl-whitepsace-only`, **8.173% under both presets**, and the whole change is
whitespace:

```
    NULL,                        /* opener */      ->      NULL, /* opener */
```

The human paints each deleted 23-space run `Delete`. Codediff paints **nothing** — five runs, both
presets, identical dumps. The runs sit *between* two unchanged sibling tokens (`NULL,` and the
comment), which is gap text: `diff::text` derives its ranges from node spans, and a run that belongs
to no node has nothing to hang a verdict on. *(Mechanism read from the dump plus `own_content_span`'s
own doc comment on gap handling — not traced end to end.)*

Why this never appeared before: `cpp-whitespace-only-change` has been in the corpus for a long time
but has **no painting** (`text_mappings: 0`), so this is the first painted whitespace-only fixture.
The consequence is worth stating plainly — **a whitespace-only commit currently renders as no
change at all.** Whether that is a defect or the correct reading of "never paint whitespace" is a
product decision, not a bug report; it is filed here because nothing else in the corpus asks it.

### F. NEW (narrow) — a lone `\r` painted `Insert` on a CRLF file

`c-microsoft-terminal-add-two-includes`, **0.360%, FULL only**, and the entire disagreement is one
byte: `side=1 row=5 bytes=141..142 ours=Some(Insert) theirs=None text="\r"`.

Smallest finding in the set, and deliberately *not* generalised: 22 corpus fixtures have CRLF line
endings, 3 of them are painted, and the other two
(`javascript-microsoft-typescript-add-use-strict`, `-2`) are exactly 0.000% under both presets. So
this is not "CRLF is broken" — it is one shape (an inserted line in a CRLF file) that `MINIMAL`
handles and `FULL` does not. Symptom verified; mechanism not traced. `trim_trailing_whitespace`
uses `char::is_whitespace`, which *does* match `\r`, so the naive explanation is already ruled out.

## Verified vs. read once

Verified directly against the fixture source or the code: E's whitespace shape (read the two files),
E's "first painted whitespace-only fixture" claim (checked `cpp-whitespace-only-change`'s mapping),
F's non-generality (enumerated every CRLF fixture and measured the painted ones), D's preset
inversion (read both dumps for the same fixture).

Read once, from the `painting_disagreement_detail_batch` dump: every per-fixture family assignment
above. All 27 disagreeing fixtures were dumped and read, but each by one pass, and the family
boundaries between A and C in particular are judgement calls on fixtures that show both.

## Resolved: the goal now covers the whole corpus

`handmade_painting_disagreement_report` scanned `diffs/handmade` alone, which under-reported the
moment 84 stratified fixtures became painted. It is now **`painting_disagreement_report`**, scans
every dataset, and prints three aggregates:

```
whole corpus                 149 fixtures    17510 / 2371356  bytes =  0.7384%   (goal: < 1%)
excluding parse errors       141 fixtures    16624 / 2210310  bytes =  0.7521%
handmade only (historical)    57 fixtures    12720 / 1531982  bytes =  0.8303%
```

**Widening the goal improved the number**, 0.8303% → 0.7384%, and it still clears `<1%`. The 86
fixtures censused above are small and mostly exact; byte-weighted against handmade's 1.53M painted
bytes they pull the rate down rather than up. The alarming 3.767% / 4.398% pooled over those 86 is
real but is a rate over 53KB, not over the corpus.

### The parse-error bucket is not the family you want

The exclusion is derived — `Node::has_error` on either side's root, no hand-maintained list — and
it flags 8 fixtures. **It does not catch family B.** Checked directly: three of the four
Go-template-in-`.html` fixtures parse *clean*, because tree-sitter-html reads `{{ ... }}` as
ordinary text and reports no error at all. The 8 it does flag are mostly ordinary C headers whose
macros tree-sitter-c stumbles on, and they are *better* than corpus average — which is why
excluding them **raises** the rate slightly. That inversion is the useful signal here: parse
failure is not what drives painting disagreement, and a metric named for it will not isolate the
mis-detected-language problem. Family B needs language detection, and no `has_error` test finds it.
