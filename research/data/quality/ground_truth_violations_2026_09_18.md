# Ground-truth self-contradictions, 2026-09-18

Every fixture's hand-authored ground truth checked against **itself** — no `diff_code`, no
codediff output — by the seventeen rules in `src/test/helper/human_mapping/invariants.rs`.
Regenerate with:

    cargo test --release --lib --features test-fixtures invariant_violations -- --ignored --nocapture

**63 violations over 17 of 1,134 solved fixtures (1.5%).** Each one is data whose author would not
defend it if it were pointed out, so each is a fixing job rather than a judgement call. Ordered
below by whether the fix is mechanical.

## What this does *not* cover

Every rule here is **intra-fixture**: it asks whether one fixture's mapping and paintings agree
with each other. None of them compares two fixtures, so a pair of fixtures whose paintings imply
*opposite conventions* for the same shape is invisible to all seventeen. That question is open, and
it is the harder one: it needs a shape classifier over painted runs before two fixtures can be said
to disagree about the same thing. See `src/diff/TODO.md`'s 2026-09-17 painting entry for the one
case that prompted it and why that case turned out not to be evidence.

## By rule

| # | rule | violations | fixtures | mechanical? |
| --- | --- | ---: | ---: | --- |
| 16 | `identifier_updates_are_painted_by_preset` | 30 | 9 | yes — repaint the identifier to the preset's own reading |
| 1 | `rows_end_on_visible_characters` | 12 | 3 | yes — shorten the run off the trailing space |
| 9 | `move_against_unmatched` | 8 | 1 | no — the two ground truths disagree about 3 bytes |
| 11 | `removed_leaves_are_painted` | 6 | 2 | no — 84 leaves, mostly one fixture |
| 3 | `delimiter_pairs_agree` | 3 | 1 | no — the mapping splits three bracket pairs |
| 10 | `paired_leaves_are_not_deleted_and_inserted` | 2 | 1 | yes — one `;` |
| 12 | `edited_leaves_are_painted` | 1 | 1 | no — same fixture as 3 and 11 |
| 4 | `full_paints_a_wholly_changed_line_whole` | 1 | 1 | yes — extend the run over the indentation |

Nine of the seventeen rules fire nowhere: 2, 5, 6, 7, 8, 13, 14, 15, 17.

## By fixture

| fixture | n | rules | what is wrong |
| --- | ---: | --- | --- |
| `java-defects4j-cli-1-commandline` | 8 | 9 | Both paintings call 3 bytes `Move` (before row 93, after row 91) that the tree mapping leaves unmatched. One of the two is wrong; a `Move` says the code survives and an unmatched node says it does not. Fires four times because the fixture carries four paintings (`Minimal`/`Full` × `inner`/`outer`). |
| `cpp-add-templates` | 6 | 16 | `Minimal` paints the whole of `Box`/`IntBox` on rows 3/4, 6/7, 11/12. The differing word is `Int`; a `Minimal` reading leaves the rest of the identifier alone. |
| `java-defects4j-chart-22-keyedobjects2d` | 6 | 1 | Both paintings end a run on a trailing space on after rows 321, 332, 368. CRLF file — the run stops on the space before the `\r`. |
| `rust-rust-lang-rust-change-use` | 6 | 16 | As `cpp-add-templates`. |
| `rust-turbopack-module-rule` | 6 | 3, 11, 12 | The worst of the seventeen, and three separate contradictions. The mapping calls `{` on row 201 inserted and its `}` on row 276 matched (same for 203/220 and `(` 204/219); `Minimal` leaves 81 after-side leaves unpainted that the mapping says are gone; and it paints neither side of an edited leaf the mapping says changed. |
| `java-defects4j-cli-18-posixparser` | 4 | 11 | Both paintings leave the identifier `token` on row 128 unpainted on both sides, while the mapping deletes and re-inserts it. |
| `java-defects4j-jacksondatabind-94-subtypevalidator` | 4 | 1 | Both paintings end a run on the trailing space of a comment (before row 103, after row 106). |
| `typescript-add-generics` | 4 | 16 | `Container`/`NumberContainer` on rows 1 and 17. |
| `c-ladybirdbrowser-ladybird-change-to-a-different-class` | 3 | 16 | `WebGL2RenderingContext`/`WebGLRenderingContextBase` on rows 19, 26, 28. |
| `c-ladybirdbrowser-ladybird-move-to-a-different-class` | 3 | 16 | Same three rows, same pair — the two fixtures share a source file. |
| `java-defects4j-mockito-21-constructorinstantiator` | 3 | 16 | |
| `java-defects4j-cli-14-groupimpl` | 2 | 10 | Both paintings delete a `;` on before row 262 and insert one on after row 258 that the mapping pairs as the same text. |
| `java-defects4j-cli-24-helpformatter` | 2 | 1 | Trailing space, before row 825, both paintings. |
| `kotlin-nextcloud-android-rename` | 2 | 16 | `Abi`/`CfgAbi`, rows 19 and 21. |
| `rust-tauri-apps-tauri-rename-mod` | 2 | 16 | The *other* direction: `Full` paints only part of `v2_rc`/`v2_beta` on row 6, where a `Full` painting marks a renamed identifier entire on both sides. |
| `java-defects4j-cli-16-groupimpl` | 1 | 4 | `Only one solution` paints every visible character of after row 92 `Insert` but leaves its 12 columns of indentation unpainted. |
| `rust-algorithm-change` | 1 | 16 | |

## Reading of the list

**Invariant 16 is half of everything and is one repair repeated.** Nine fixtures, 30 violations,
and every one is a renamed identifier painted at the wrong granularity for its preset — `Minimal`
painting the whole identifier where only the differing word changed (eight fixtures), or `Full`
painting only the differing word where it should mark the identifier entire
(`rust-tauri-apps-tauri-rename-mod`, the only one going the other way). None of them needs a
judgement about what the change *is*; the rule already computes the differing words and says so in
its message. The two `ladybird` fixtures are the same three rows of the same file, so eight
distinct repairs cover all nine.

**Invariant 1 is twelve messages and four real spots.** Each fires once per painting, and every
fixture here carries two, so `chart-22`'s three rows, `jacksondatabind-94`'s two and
`cli-24`'s one are six distinct edits — shorten the run by one space.

**`rust-turbopack-module-rule` should be re-derived, not patched.** It is the only fixture
carrying three different contradictions, its invariant-11 message names 81 unpainted leaves the
mapping says are gone, and it is also the corpus's largest painting disagreement (4,177 bytes
`FULL`, 3,032 `MINIMAL`) and one of its larger mapping residuals. A fixture whose two ground truths
disagree at that scale is not grading anything.

**`java-defects4j-cli-1-commandline` is three bytes and a decision.** Invariant 9 is the only rule
that crosses the two ground truths on survival, and here they flatly contradict: the painting says
those three bytes moved, the mapping says they were deleted and re-inserted. Someone has to say
which, once, and both then follow.
