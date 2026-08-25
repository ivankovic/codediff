# Which node "moved"? A choice the mapping does not make

**Authored, not generated.** A design decision and the reasoning behind it, recorded here because
the reasoning is the interesting part and it survives the numbers. The numbers below are dated and
re-measurable; the argument is what the paper should carry.

## The observation

A tree mapping says which node on the before side corresponds to which node on the after side. It
does **not** say which of them moved. That sounds like a distinction without a difference, and it
is not: for a reorder, "moved" is a property of the *presentation*, and the mapping is indifferent
between the available presentations.

Concretely. A Python file's import block, before and after:

```
 24  import packageurl                             24  import license_expression
 25  import requests                               25  import packageurl
 26  import saneyaml                                26  import requests
 27  import utils_pip_compatibility_tags            27  import saneyaml
 28  from commoncode import fileutils               28  from commoncode import fileutils
 29  from commoncode.hash import multi_checksums    29  from commoncode.hash import multi_checksums
 30  from commoncode.text import python_safe_name   30  from commoncode.text import python_safe_name
 31  from packvers import tags as packaging_tags    31  from packvers import tags as packaging_tags
 32  from packvers import version as ...            32  from packvers import version as ...
                                                    33
                                                    34  import utils_pip_compatibility_tags
```

`import utils_pip_compatibility_tags` was relocated below the `from ...` block. Every one of these
nine imports is matched to its counterpart, identically, and the mapping is not in doubt anywhere.
Yet there are two entirely faithful renderings of that same mapping:

* **one line moved** past five that stayed put, or
* **five lines moved** past one that stayed put.

Both descriptions account for exactly the same set of matched pairs. Both are consistent with the
mapping. Neither is derivable from it. A tree edit distance is equally indifferent: the mapping's
cost is identical under either reading, because cost is a function of the pairing and both readings
*are* the same pairing. The cost model has nothing to say here, and cannot be extended to say
anything, because there is no difference in the object it scores.

This is the general point, and it is the one worth making in the paper: **the edit script is not
the visualization.** A minimum-cost mapping is the answer to a question a reader did not ask. What
a reader wants to know is "what did someone do to this file", and for a whole class of edits -
reorders, extractions, rewraps - several accounts of "what someone did" are equally faithful to a
single mapping. Choosing among them is a decision the matching literature does not address, does
not equip a tool to make, and yet every tool that renders a diff has to make.

## How the two accounts arise here, mechanically

`diff::text::TextDiff::from` builds each side's ranges by walking that side's tree in its own
order, and `ranges`' `crossed_backwards` test asks whether a matched node's destination lands
*behind* the walk's running anchor. That is a question about walk order, not about the pair - so
the two walks answer it differently:

* Walking **before -> after**, the mover is met first, at line 27. It matches forward to line 34
  and advances the anchor there. The five `from ...` imports that follow then have destinations at
  lines 28-32, behind that anchor: the walk flags **five**.
* Walking **after -> before**, the five imports are met first and match in order, never running
  backwards. Only the mover lands behind: the walk flags **one**.

Each walk is self-consistent. Neither is wrong on its own terms. They simply answer "who broke the
order" relative to different traversals, and the answer is not traversal-invariant.

Left unreconciled, this is visible as a bug rather than as a philosophical point: the two sides of
the diff highlight different nodes, so following a highlighted move to its counterpart lands the
reader on an *unhighlighted* node. That is how it was found - a user pressed enter on a highlighted
before-side import and arrived at a correctly-aligned but unpainted after-side one.

## Why the obvious reconciliations both fail

* **Intersection** (a pair is a move only if both walks agree) is empty here - each pair is flagged
  by exactly one walk - so the reorder renders as no change at all. That is the exact regression
  `crossed_backwards` was added to prevent: before it existed, a pure sibling reorder produced no
  non-`Identical` range anywhere and the diff claimed the file was untouched.
* **Union** paints all six imports. It is symmetric, and it is worse than either walk alone: it
  tells the reader that six things moved when one did.

The reconciliation has to *choose*, and the choice needs a criterion the mapping does not supply.

## The decision: keep the smaller account

**Where the two walks disagree, believe the one that blames fewer rows.**

Relocating one line past five is one move, not five. The criterion is description length: among
renderings faithful to the same mapping, prefer the one that asserts the least. It is the same
instinct behind preferring a short edit script to a long one, applied one level up - not to the
mapping, which is already fixed, but to the account given of it.

It is also, and this matters more than the principle, the reading that matches how the edit is
described by the person who made it. Nobody writes "I moved five imports up"; they write "I moved
that import down". The smaller account is the one a commit message would contain.

Implemented as `diff::text::reconcile_moves`, run between the two directional walks and the merge
step. Agreed pairs are never touched, so a file whose walks already agree is unaffected.

### What it is not

It is not a claim that the smaller account is *true*. Both accounts are true. It is a claim that
the smaller one is more useful, which is a different and weaker thing, and the paper should say so
in those terms. There is no ground truth to appeal to here: the human mappings this project scores
against record *correspondences*, not attributions of movement, so they cannot adjudicate this and
were never asked to.

### Known limits, stated rather than discovered later

* **Per file, not per reorder.** A file containing two independent reorders that disagree in
  opposite directions gets one global verdict, and the minority one is decided wrongly. Grouping
  disagreements into per-reorder clusters needs a notion of which reorder a pair belongs to that
  nothing in the pipeline currently has.
* **Ties go to the before side.** Arbitrary, but it has to be a fixed side or the output stops
  being a function of the input. The case it reaches is two accounts of equal size, where neither
  is more economical.
* **Promotion needs an exact extent lookup** into the other side's range list, which is not quite
  total. A counterpart that isn't found stays unpainted rather than being invented.

## Prevalence, 2026-08-25

Measured over the 501 corpus fixtures with a before/after pair, by pairing every before-side `Move`
against its own `destination` and asking what the other side calls that exact extent:

| | before `reconcile_moves` | after |
|---|---|---|
| fixtures with >=1 unpaired move | 24 | **5** |
| unpaired move ranges | 67 of 1538 (4.4%) | **12 of 1508 (0.8%)** |
| codediff line mismatches vs the human mappings | 4890 | **3392** |
| fixtures with zero line mismatches (of 500) | 427 | **431** |

Concentrated rather than pervasive before the fix: one fixture
(`vimscript-neovim-neovim-awful-test-case-bunch-of-hex-colours-more-data-than-code`) accounted for
25 of the 67, and the rest ran 1-6 each. That outlier is gone afterwards; what survives is 6 in
`ruby-jmespath-jmespath-formatting-and-style-guide-fixes`, 3 in `rust-next-font-imports-generator`
and 1 each in three others - which is the first documented limit above showing up exactly where it
would be expected, since a whole-file restyle is the many-independent-reorders shape a single
per-file verdict cannot serve.

Note the move-range total falls too, 1538 -> 1508: the before side stops claiming five ranges where
one will do.

**On the line-agreement figure, and how much weight it can carry.** The 31% drop in line mismatches
is real and was measured both ways on the same corpus (the pre-fix run reproduces 24/67/1538
exactly, so the two runs are comparable). But it is *weak* evidence for this decision rather than
strong, and the paper should not lean on it. The human mappings record **correspondences**, not
attributions of movement; they were never asked which node moved, and cannot adjudicate that
question. What the metric actually rewards here is narrower and less interesting than the decision:
`line_operations` counts a `Move` row as touched and an `Identical` row as not, so blaming one row
instead of five removes four rows that the human mapping - correctly - does not consider changed.
The improvement confirms the smaller account marks less untouched text as changed. It does not
establish that the smaller account is the *right* account, and nothing in this corpus can.

**A methodological note worth carrying into the paper too.** The first attempt to size this
compared, per fixture, the *number of rows* each side marked as moved, and reported 63 fixtures
(12.6%) disagreeing. That proxy overstated the real figure by 2.6x, because a move's row span can
legitimately differ between the two sides without the two sides naming different pairs. Two
fixtures it flagged most dramatically - `go-henri-gasc-cliphist-auto-generated-file` at 3 rows
versus 1194, and `java-hunterhacker-jdom-move-a-block` at 31 versus 4 - turned out to have one and
zero genuinely unpaired moves respectively. The exact measurement pairs each range against its own
recorded `destination`; the proxy compared aggregates and was wrong about which fixtures were even
affected, not just by how much.

The asymmetry also has a direction, which is what the mechanism predicts: **zero** fixtures had
moves on the before side only, and two had them on the after side only. The before -> after walk
tends to blame the block that was jumped over, which is usually the larger of the two.

## Reproducing

The fixture above is
`src/test/data/samples/python-x-aboutcode-org-license-expression-af87cfab-utils_thirdparty` (a
sample, never promoted to `diffs/`, so it is *not* among the 501 counted here). Rendered move
hunks, via `codediff --mode json before.py.test after.py.test`:

```
before reconcile_moves:   before: move rows 27..32     after: move rows 33..34
after  reconcile_moves:   before: move rows 26..27     after: move rows 33..34
```

Both sides now name the mover, and `from commoncode import fileutils` is no longer highlighted at
all.
