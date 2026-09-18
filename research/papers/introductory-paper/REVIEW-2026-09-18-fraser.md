# First review by the second author — 2026-09-18

Gordon Fraser read the 12-page PDF of the same day (after the red-pen pass recorded in
`REVIEW-2026-09-18.md`) and sent six comments. He joined the author list in the same commit.
This file records each comment, what was decided, and what changed in the paper and in the
analysis code, so the reasoning is not lost once the text has moved on.

**STATUS: all six applied. One of them (F1) replaced two tables and two figures with two new
figures and needed a corpus re-measurement, recorded at the foot of this file.**

---

## The comments

> Looks good already. The many RQs and groups are indeed a bit unorthodox, but I think they can
> work, just need a bit more glue to provide smoother flow.

| # | comment |
|---|---|
| F1 | Is there some coherent way you could visualise the individual distributions? Might be easier to interpret compared to the combination of p50/p90/p99/max tables and the 4 different plot styles used for one plot only each. |
| F2 | The tool currently feels a bit disconnected to all the empirical work; you name 2 design targets and that's the only obvious link with all the empirical results. Without thinking deeper about the 5 phases I struggle to conceptually understand what's new/different about CodeDiff, and how the things done in the phases relate to what you empirically found. |
| F3 | The fact that AST-based is better than line-based diffing, is that sufficiently established we can just take it for granted? |
| F4 | The paper could maybe do with a bit more discussion of why diff quality matters, you only implicitly hint at that by hinting that diff quality matters for ease and quality. Maybe that's an opportunity to make better use of the "groups" — each of the groups has an underlying different reason for why that aspect matters. Maybe instead of just listing groups you could sort of structure it around why it matters and that we thus need to understand to what degree each aspect is affected? (Might not need to use the term "group" at all for that.) |
| F5 | It might not harm explaining more explicitly what "solving a diff by hand" means (3.3). |
| F6 | The CodeDiff results feel very Claudey and I honestly have no idea what the conclusion is. |

---

## F4 and F2 together — one spine for the paper

These two comments solve each other, so they were applied first and as one change.

**Introduction.** The "four groups of research questions" list is gone. In its place the
introduction says what a reviewer does with a diff, and derives four properties a diff has to
have, each with its reason and the questions that test it:

| property | why it matters | tested by |
|---|---|---|
| Expressible | if the true correspondence cannot be written in the tool's operations, no search finds it | RQ1.1 |
| Agreed | where several correspondences exist the diff must pick the reader's, which is not always the cheapest | RQ1.2, RQ3.1 |
| Timely | a correct diff that arrives late is one the reader stopped waiting for | RQ2, RQ4.2 |
| Faithful on screen | the reader sees text, not nodes; the rendering must say what the mapping says | RQ3.2, RQ3.3 |

RQ4.1 then asks whether the tools that exist have those properties. The RQ numbers are kept,
since the result boxes RA1.1–RA4.2 and the cross-references in Sections 9 and 10 are wired to
them; the term "group" no longer appears. Each question is now stated in full only in the section
that answers it, and the source comment that used to require the intro list and the sections to
be verbatim copies now requires the numbers to stay in step instead.

**Glue.** Every measuring section opens with one sentence naming the property it tests and why:
Section 4 (Expressible and Agreed, on the human solutions alone), Section 5 (Timely, on the exact
algorithm alone), Section 6 (Agreed and Faithful on screen, where the tool shows one answer and
more than one is correct), Section 7 (whether the tools that exist have the four properties).

**Section 8.** The opening now says what is new in plain words: not a matching algorithm, since
every technique in CodeDiff is published, but the order in which they run and what each is
allowed to decide, with every step a consequence of a measurement above. A new Table 6,
"What each empirical result decided in CodeDiff's design", lists the correspondence:

| finding | consequence |
|---|---|
| median edit rewrites 3.2% of its file (§3.2) | Phase 1 hashes the unchanged majority first |
| whole-tree APTED completes within 1 s for 54.5% of code pairs (RA2) | APTED runs only inside one matched pair (Phase 3) |
| human mapping costlier than the cheapest on 3.9% (RA1.2) | Phases 2 and 3 encode reader preferences that cost more than the optimum |
| 11.5% inexpressible, 1.8% tie at different mappings (RA1.1, RA3.1) | the phase order is the tie-break |
| 32% of nodes never reach the screen (RA3.3) | accuracy reported on visible nodes and rendered lines |
| no tool renders more than 59% perfectly (RA4.1) | the bar the evaluation must clear |
| files reach 905,004 nodes (§3) | the Robust target |

The two design targets survive, introduced as "two of those consequences are numeric targets",
and the Fast target now says it is the Timely property's one second applied to the commit
population. Phases 2, 3 and 5 each close with one sentence naming the result they apply.

## F6 — the CodeDiff results

Agreed. The old Robustness/Accuracy/Speed/Summary block stacked seven numbers and two
digressions with no claim in front of them. It is now Result / Accuracy / Robustness / Speed /
Limits:

- **Result** is one sentence stated first: 89% of the corpus rendered perfectly against 59% for
  the best established configuration, at a 3.2 ms median including the parse, every fixture
  completed, and it falls short where every tool falls short.
- **Accuracy** keeps the numbers but in one paragraph with the by-dataset table under it.
- **Speed** reads CodeDiff's curve off the new Figure 3 rather than restating Table 5.
- **Limits** says what the result does not show: the 1,316 ms maximum is past the 1,000 ms
  target; the Robust ceiling is stated, not exercised; CodeDiff is timed in-process while every
  other tool pays process start-up.
- The digression about which dataset every *other* tool scores best on moved to Section 7, under
  Figure 5, where it belongs.
- The Summary paragraph, which restated the figures, is gone.

## F5 — what solving a diff by hand means

Section 3.3 now says it. A solution is, for every node of the before-AST, which node of the
after-AST it corresponds to or that it has none: the same object an algorithm produces, decided
by a person. The annotator works in `src/bin/human_solver`, a terminal tool published with the
corpus that shows the two trees side by side; it classifies a matched pair itself (identical,
updated, matched with differing children) by comparing text and can pair byte-identical subtrees
in one keystroke, and everything else is marked by hand from an empty mapping, not from any
tool's proposal. The solution is one JSON file per change next to the two versions and a
provenance README. The criterion (what a reviewer would want shown), the handling of ties (both
recorded) and of inexpressible changes (closest solution plus a note), and the painting are each
stated in one sentence. The description was checked against the tool's own doc comment and
against a fixture on disk before it was written.

## F3 — is AST better than line-based a given?

No, and the paper now says so. The introduction states that we do not take it as given, that
RQ4.1 measures it with line-based `diff` scored on the same basis as every AST-aware tool, and
that Section 7 reports where each kind does better. The paper's own data is the reason to be
careful: `diff` is the fastest configuration at every percentile, and every line-granularity
tool scores its best on Defects4J, whose bug fixes are the short localised edits a line diff
recovers most reliably. Figure 1 stays as the motivating failure mode, not as the argument.

## F1 — one way to draw every distribution

The paper used four plot styles on four figures (horizontal bars, grouped bars, stacked bars, a
violin) and three p50/p90/p99/max tables. Two of the figures are categorical and stay as bars.
Everything else was the same question — what share of the population sits at or below x — and is
now drawn that way: an empirical cumulative distribution on a log axis, the paper's percentiles
dotted on the curve, and the one-second budget as a dashed line wherever a budget exists, so the
crossing is the number the paper quotes.

| before | after |
|---|---|
| Table 1 (file size percentiles) and Table 2 (edit size percentiles) | Figure 2, *corpus shape*: six cumulative curves, file size in lines / bytes / nodes over the top, edit size in lines per file / lines per commit / share rewritten below. The percentiles survive in the prose as macros. |
| Figure 3 (RQ2 grouped bars by size bucket and category) | Figure 3 left panel: completion time per category; the height at the budget line *is* RA2. The bucket numbers stay in the prose and in RA2's box. |
| Figure 6 (runtime violin) | Figure 3 right panel: every configuration over every repeated run, CodeDiff charged for its parse, parse alone dotted as the floor. Table 5 stays as its numeric companion, since it carries the Max column. |

Both figures come from `analysis/distributions_report.py` (`make distributions-report`), which
reads only committed artifacts. Two producers gained a value-to-count distribution output so that
is possible: `analysis/file_stats.py` writes `data/corpus_stats/code_file_size_distribution.csv`,
and `analysis/edit_shape_stats.py` writes `data/corpus_stats/edit_shape_distribution.csv`.
Neither existed before, because the edit walk deliberately kept no per-edit records (the class
doc comment explains the 7.7 GB reason); value-to-count is the committable form.

### The measurement behind the edit-size row

The file-size row needed only a re-read of the corpus database (`make file-stats-report
MODE=full`, 2026-09-18). The edit-size row needed the corpus walk itself re-run, since the
previous run's distribution was never written out: `edit_shape_stats.py` over the Full corpus at
depth 50, 15:50 to 17:47 on 2026-09-18. Its aggregate outputs came back byte-identical to the
committed `edit_shape.csv` and `variables_edits.tex`, so no number in the paper moved and the
distribution is exactly the population behind the percentiles the prose still quotes. The run is
recorded in `data/corpus_stats/PROVENANCE.md`.

Every dot on the new figure was checked against the macro it stands for: file sizes against
`\LocPFifty` and friends, edit sizes against `\EditsLinesPerFilePFifty` and friends, and the RA2
crossings against `\RqOneCodePct`, `\RqOneScriptingPct` and `\RqOneConfigDataPct`.
