# What painting text ground truth by hand taught us

**Authored, not generated.** Findings from painting the first 26 fixtures' text ranges by hand in
`human_solver`'s `t` view (see `HumanTextMapping`). Written down because they are the rules a
painter converges on after a few dozen fixtures and would otherwise have to rediscover - and
because two of the three are precise enough to enforce, which makes them worth more than advice.

Every number below is measured over what was actually painted, not estimated. Re-derive them by
reading `text_mappings` out of `src/test/data/diffs/*/*/human_mapping.json`.

## The corpus these come from, 2026-08-26

26 fixtures painted: 10 carry both a `Minimal` and a `Full` painting, 16 carry a single
`Only one solution`. 275 entries in total - 91 `Match`, 58 `Delete`, 126 `Insert`. Of the matches,
66 resolve to a move (both sides byte-identical) and 25 to an update. 8 of the 91 are N:M rather
than 1:1 - five 3:1 and three 2:1 - which is roughly one match in eleven, and the reason
[`HumanTextEntry`] carries a list of spans per side at all.

## 1. There are two coherent painting styles, and the difference is punctuation

The `Minimal`/`Full` split is not a matter of care or thoroughness. It is a real fork: the same
edit has a tight rendering and a generous one, and both are defensible.

* **Minimal** leaves indentation, brackets and other pure punctuation unpainted, marking only the
  content that carries meaning.
* **Full** paints them, so that every byte whose role changed is accounted for.

Measured across the 10 fixtures painted both ways: **Minimal 88 entries, Full 115**. Of the 27
entries `Full` adds, **16 are a single punctuation token** - eight `(`, five `)`, two `):` and one
`;`. The remaining 11 are ordinary content spans a `Full` pass split more finely.

So the styles differ by a little over a quarter in entry count, and more than half of that
difference is brackets. That is worth knowing before deciding a diff is "wrong": a tool that leaves
a bracket unpainted disagrees with `Full` and agrees with `Minimal`, and both paintings are on
file precisely so that is not scored as an error. See `move_attribution.md` for the same point
made about a different degree of freedom.

## 2. Brackets share fate, and this holds without exception

A bracket pair is one construct. If the opening bracket is painted, the closing one should be too,
under the same operation - and if one is left alone, so is the other. The intuition is obvious once
stated; what makes it useful is that it turns out to be *absolute*.

Checked over every `(`/`)` pair in every painted file, on both sides, for every named painting:

| | |
|---|---|
| pairs where both brackets share the same painted/unpainted fate | **426** |
| pairs split - one painted, the other not | **0** |

Zero exceptions in 426 opportunities. That is strong enough to be a lint rather than a guideline:
a split pair is almost certainly a slip of the selection, and flagging it would catch a class of
error a human cannot see by eye in a large fixture.

**The one case where it should legitimately break** is code that was already unbalanced - a fixture
whose edit adds or removes a stray bracket, where the pairing itself is what changed. No such
fixture is in the painted set yet, so the rule is currently untested against its own exception. A
lint should therefore *report* a split pair rather than refuse to save one.

## 3. An identical line tail stays unpainted, even when the line moved

When a change happens early in a line and the rest of the line is unchanged, the unchanged tail is
left out of the painted range - even though the tail's row and column both shifted. The painter is
recording what a reader perceives as changed, and nobody perceives an untouched clause at the end
of a line as having been edited because something in front of it grew.

Measured over the matches whose spans end exactly at end-of-line on both sides:

| | |
|---|---|
| such matches | 16 |
| of those, updates whose two sides share a trailing run of >2 characters | **1** |

The single exception is `java-add-exception-handling`'s `Full` painting, where a 34-character
trailing line (`        return content.toString();`) sits inside a larger multi-line span - which is
`Full` doing exactly what `Full` is for.

**A measurement trap worth recording, because the first attempt fell into it.** Scoring this as
"the two sides share a common suffix" rather than "the line tail is identical" reports 16
violations instead of 1. All 15 of the extra ones are identifier renames where the shared suffix
falls *inside a word*: `Box` against `Box<T>` shares `Box`, `calculateArea` against `area` shares
`rea`, `calculatePerimeter` against `perimeter` shares `erimeter`. Splitting a painted range at
`rea` would be absurd, and none of those are what the rule is about. The rule is about **line
tails**, and a check that does not anchor on end-of-line measures something else and cries wolf.

## What this suggests building

In rough order of value against effort:

1. **A split-bracket lint** in the solver - report, don't block, per the exception above. Rule 2 is
   exceptionless over 426 samples and invisible to the eye, which is the ideal profile for a check.
2. **Paint both styles by default on ambiguous fixtures.** The 16 single-solution fixtures are the
   ones where the painter judged the answer unique; the 10 two-solution ones are where they did
   not. That judgement is itself data, and it is already recorded in the solution names.
3. **A line-tail hint**, much more cautiously. Rule 3 holds 15 times in 16, but the trap above
   shows how easily a naive version misfires - and at one exception in sixteen it would be firing
   about as often as it is right.
