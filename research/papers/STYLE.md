# Writing style for Marko Ivanković's papers

Instructions for an LLM agent drafting or revising academic prose in this directory. Derived by
reading 12 of Marko's published papers (PDFs archived in `/var/tmp/papers`, converted to text in
`/var/tmp/papers/txt`, both outside this repo) and measuring what actually recurs, not from a
general impression of "good academic writing." Every rule below is backed by a frequency count or
a direct quote from that corpus; see "Evidence" at the end for the raw numbers.

Papers sampled: Code Coverage at Google (2019), State of Mutation Testing at Google (2018),
Practical Mutation Testing at Scale (2021, TSE + arXiv), Does Mutation Testing Improve Testing
Practices? (2021), An Industrial Application of Mutation Testing (2018), AI-Assisted Assessment of
Coding Practices (2024), Productive Coverage (2024), Please Fix This Mutant (2023), MuRS (2023),
What Types of Automated Tests do Developers Write? (2025), plus two earlier (2011) web-browser
communication papers pre-dating the Google work. The 2011 papers read differently (no em-dashes, no
RQ/RA structure) and are treated as out of scope below — the rules describe the 2018-2025 body of
work, which is stylistically consistent with itself.

## Register

- Formal throughout. **Zero contractions** across the entire corpus (checked directly: grep for
  don't/doesn't/isn't/can't/etc. across all 12 papers returns 0 hits). Never write "doesn't",
  "it's", "we're" — always "does not", "it is", "we are".
- First-person plural, active voice, not passive. "We" appears roughly 200 times across the
  corpus. Prefer "We evaluate X" over "X was evaluated" or "This evaluates X."
- No rhetorical hooks, no scene-setting anecdotes. Sections open with a plain, definitional or
  factual sentence that establishes shared ground before any claim is made. Example (opening line
  of a Background subsection): *"Mutation testing, a process of inserting small faults into
  programs and measuring test suite effectiveness of detecting them was originally proposed by
  DeMillo et al."* Not: an evocative framing sentence about the problem's importance.
- Hedge precisely, scoped to what the data shows, and state null results as results, not as
  failures to hide. *"We found no evidence that showing code coverage during code review improves
  coverage more than the improvement already provided by the code review process itself."*
  *"In summary, we did not find evidence [that mutation testing changes X]."* A negative or null
  finding gets the same declarative, unapologetic sentence form as a positive one.

## Punctuation

- Em-dashes ("—", U+2014, unspaced) are used for a genuine appositive aside — *"a novel approach to
  code coverage—termed Productive Coverage—that automatically generates..."* — not as a
  general-purpose joiner. Frequency varies by paper (0 in the two 2011 papers and in State of
  Mutation Testing at Google 2018; 11-23 in most 2021+ papers), but it is always local: one aside,
  bounded by a matching pair, not a chain of clauses.
- **Do not use a spaced hyphen (`word - word`) as an em-dash substitute.** This does not occur
  anywhere in the corpus. If you want a dash, write the actual em-dash character with no
  surrounding spaces.
- Do not chain more than two independent clauses in one sentence with dashes or semicolons as a
  substitute for separate sentences. The corpus favors shorter, separately-punctuated declarative
  sentences over long compound ones, even when covering a complex point — see the DeMillo et al.
  example above, which is one clause, then a second short one, not a single long sentence carrying
  both.

## Sentence structure

- Sentences run long and information-dense — roughly 25-30 words on average in clean prose
  (measured on abstracts/introductions, which extract cleanly from the two-column PDF layout;
  table- and reference-heavy pages extract too noisily to trust for this). Length comes from
  packing in a second or third qualifying clause, not from padding — cut a long sentence by
  removing a clause entirely, not by inserting filler to shorten it elsewhere.
- **Definitional appositive opener**: introduce a term with a comma-set appositive naming what it
  is, then continue the sentence, rather than a separate defining sentence. *"Mutation testing, a
  process of inserting small faults into programs and measuring test suite effectiveness of
  detecting them, was originally proposed by DeMillo et al."* Use this the first time a paper-level
  term is introduced in a section, not as a decoration on every noun.
- **Colon-introduced lists and explanations** are common for both enumerated items ("this paper
  makes the following contributions:") and single-clause elaborations ("Probabilistic. For each
  line, at most one mutant is generated:"). Prefer a colon over a dash or a new sentence when what
  follows directly enumerates or unpacks what precedes it.
- **Bullet and RQ list items are grammatically parallel and each a complete sentence**, ending with
  a period, not a fragment. Contribution bullets each open with the same subject ("It
  demonstrates...", "It details...", "It reports..." — all same paper, same verb tense, same
  subject "It" referring to the paper). Do not mix fragment bullets with full-sentence bullets in
  the same list.
- **Limited, deliberate framing-verb vocabulary**, not a thesaurus-varied one. For describing what
  the paper itself does: "This paper presents/describes/proposes/reports/details/investigates."
  For describing what the authors did: "We propose/present/describe/report/introduce/evaluate/
  study/measure/observe/design/develop/implement." For claims beyond directly-measured fact: "We
  argue" (9 occurrences — justifying a design decision or interpretation) and "We conjecture" (6
  occurrences — a plausible explanation the data is consistent with but does not itself prove,
  always flagged as such rather than stated as fact). Reach for "we conjecture" specifically when
  making this kind of unproven-but-plausible claim, rather than overstating it as "we found."
- **Bold run-in paragraph labels** for a set of parallel sub-topics within one section — a short
  bold term, a period, then the explanation continuing on the same line: *"Probabilistic. For each
  line, at most one mutant is generated..."* `main.tex`'s evaluation section already does this
  correctly (`\textbf{Dataset.}`, `\textbf{Ablation study.}`, `\textbf{Speed.}`) — keep doing it
  there and elsewhere a section has several parallel sub-topics to walk through.

## Preferred words

Measured by direct substitution-pair frequency across the corpus; prefer the left term. Google- and
Perforce-specific terminology from the corpus (changelist/CL, and the diff/patch pairing, both tied
to Google's internal review tooling) is deliberately excluded here — this paper is not about
Google's internal systems and should not import their house vocabulary. Use the general term
("commit", "diff") that fits the version-control system actually being discussed.

| Prefer | Over | Counts |
|---|---|---|
| developer | programmer | 490 vs. 8 |
| test suite (two words) | testsuite | 93 vs. 0 |
| codebase (one word) | code base (two words) | 48 vs. 31 — inconsistent in the corpus itself, so this is a mild preference, not a hard rule; pick one and hold it within a single paper |
| "Section N" / "Figure N" / "Table N", spelled in full | "Sec. N" / "§N" | 53 vs. 0 |

Other confirmed-common vocabulary, safe to use freely: "e.g." (93 occurrences) and "i.e." (50) both
appear often and are not avoided in favor of spelled-out "for example" (97) / "that is" — the
corpus uses all three, choosing whichever fits the sentence rhythm rather than banning the
abbreviations. Numbers use comma thousands-separators ("70,000", 84 occurrences); one paper (State
of Mutation Testing at Google, 2018) uses a right-single-quote separator ("70’000") throughout,
which reads as a LaTeX/locale artifact specific to that one paper's build, not a deliberate
convention — do not imitate it.

## Visual patterns (figures and tables)

- **Captions are short noun phrases, not full sentences**, capitalized first word, ending with a
  period: *"Timeline of Productive coverage deployment."* *"Distribution of file types across the
  corpus."* Never phrase a caption as an imperative or a complete subject-verb-object sentence
  describing what the reader should conclude — describe what the figure/table *is*, not what it
  *shows you*.
- **Introduce every figure and table in the surrounding prose with "Figure N shows..." / "Table N
  shows..."**, by a wide margin the dominant construction (38 of ~53 sampled intro sentences use
  "shows"; "summarizes", "illustrates", "compares", and "breaks down" each account for only 1-4).
  Default to "shows" unless the figure is specifically aggregating ("summarizes") or juxtaposing
  two things ("compares") — do not reach for a fancier verb just for variety.
- A figure or table is always referenced from prose at or near its first appearance — never dropped
  in without an in-text pointer, and never referenced only from a caption cross-reference with no
  sentence in the body pointing to it.
- `main.tex` already follows this: its captions are short noun phrases, and Table~\ref{...} and
  Figure~\ref{...} are consistently introduced with "reports" / a body sentence pointing at them —
  keep this as-is.

## Structure (section-level)

Every sampled paper (post-2018) follows the same skeleton, in this order:

1. **Abstract.** One paragraph. States the problem, the approach in one sentence, then the
   headline empirical result with an actual number ("processes about 30% of all diffs across
   Google", "512 responses, received from surveying 3000 developers"). Never closes on a vague
   claim without a figure attached somewhere in the paragraph.
2. **Introduction.** Establishes the problem, cites 1-3 sources that motivate why it matters, then
   states what this paper does. Ends with a **bulleted list of contributions**, 3-5 items, each one
   sentence, each starting "It demonstrates...", "It details...", "It reports...", or "We
   propose/found...". This bullet list appears in every paper sampled that has a conventional
   structure (Code Coverage at Google: 4 bullets; State of Mutation Testing: 2 bullets; MuRS,
   Productive Coverage, What Types of Automated Tests: same pattern). A paper without this list is
   the exception, not the norm — include it.
3. **Background** (Section 2, immediately after the introduction — not deferred). Defines terms and
   cites the prior work needed to understand the rest of the paper. This is distinct from...
4. **...Related Work**, which is its own, separate, later section (typically second-to-last, right
   before the conclusion — Section 5 of 6 in Code Coverage at Google, Section 6 of 7 in Productive
   Coverage, Section 7 of 7 in MuRS). Background covers concepts the reader needs to follow the
   paper; Related Work revisits the closest prior work now that the reader has the paper's own
   contribution in hand, to say precisely how this work differs. Do not collapse these into one
   section.
5. **Core technical/empirical sections**, numbered and named after their actual content (e.g. "3
   Probabilistic Diff-Based Mutation Testing Analysis"), not generically ("Approach", "Method").
6. **Evaluation, structured around explicit, numbered research questions.** 8 of the 12 papers use
   an RQ/RA (research question / research answer) pattern: state "RQ1: ..." before presenting any
   result, then later pair it with "RA1: ..." stating the answer in one direct sentence, e.g. *"RQ1
   Effects on testing quantity. How does continuous exposure to mutation testing affect the number
   of tests developers write?"* followed eventually by a direct answer sentence. When a paper has a
   clear set of empirical questions, prefer this pattern over prose that only implies the question.
7. **Conclusion (often "Conclusion and Future Work").** Opens by restating what the paper did, in
   past tense, plainly: *"This paper describes Google's code coverage infrastructure, how the
   computed code coverage information is visualized, and how it is integrated into the developer
   workflow."* Frequently closes with a **second bulleted list**, this one of practical,
   prescriptive takeaways for the reader, not a recap of the results: *"Based on the lessons
   reported in this paper, we recommend the following: Measure coverage automatically at critical
   points..."* Include this actionable-recommendations bullet list when the paper has practical
   lessons to offer, which is the common case for the industrial-report-style papers this corpus is
   made of.
8. **References**, dense and inline throughout the paper (bracket-numbered, "Smith et al. [12]"),
   not deferred to a single unintegrated block — citations appear in the Background, Related Work,
   and wherever a specific number or technique needs attribution.

An explicit **"Threats to Validity"** subsection appears in only 1 of the 12 papers (Productive
Coverage) — too rare to call a hard convention, but a reasonable option when a paper's evaluation
has a specific, nameable limitation worth flagging on its own rather than folding into prose (as
`main.tex`'s Robustness paragraph currently does, calling itself "a sampled, bounded-scale result,
not a claim over the full corpus" inline).

## Evidence claims: always attach a number

Every substantive claim in the corpus is backed by a concrete count, percentage, or sample size
sitting in the same sentence or the next one: "6,000 engineers... more than 13,000 code authors",
"70,000 diffs, testing 1.1 million mutants", "43% of the changelists have 10 or fewer lines". Avoid
unquantified strength words ("significant", "many", "substantial") unless a number is within the
same sentence or the immediately following one.

## What this means for `introductory-paper/main.tex`

The current draft does not follow several of these conventions, worth knowing before revising it
further: it uses a spaced hyphen as a dash 12 times (never done in the real corpus), has no
bulleted contributions list in the introduction, has no RQ/RA structure in the evaluation section,
and merges Background and Related Work into one early section instead of splitting them. This is a
factual comparison, not an instruction to rewrite the draft — apply these rules going forward, and
raise the mismatch with Marko before restructuring anything already-written.

## Evidence

Frequency counts across the 12-paper corpus (`/var/tmp/papers/txt`, generated via `pdftotext
-layout`), for anyone re-verifying or extending this guide:

| Signal | Count |
|---|---|
| Contractions (don't/isn't/can't/etc.) | 0 |
| "We " (sentence-leading) | ~200 |
| "However," | 89 |
| "In practice" | 57 |
| "Furthermore," | 31 |
| "Overall," | 22 |
| Papers with a bulleted contributions/findings list | 10 of 12 |
| Papers with explicit RQ/RA numbering | 8 of 12 |
| Papers with em-dash (—) usage | 9 of 12 (0 in the two 2011 papers and the 2018 mutation-testing paper) |
| Papers with a "Threats to Validity" subsection | 1 of 12 |
| "Figure/Table N shows" vs. other intro verbs | 38 of ~53 |
| "e.g." / "i.e." / "for example" occurrences | 93 / 50 / 97 |
| "We argue" / "We conjecture" occurrences | 9 / 6 |
| developer vs. programmer | 490 vs. 8 |
| "test suite" vs. "testsuite" | 93 vs. 0 |
| codebase vs. code base | 48 vs. 31 |
| Comma vs. apostrophe thousands-separator | 84 vs. 0 (one paper uses a stray right-quote separator once; not a convention) |
| `main.tex` draft: spaced-hyphen-as-dash occurrences | 12 |
| `main.tex` draft: bulleted contributions list | absent |
| `main.tex` draft: RQ/RA structure | absent |
