# research/external

Third-party datasets we score CodeDiff against, and the scripts that fetch them. Nothing here is
part of the product, and nothing here is vendored: every script downloads into
`/var/tmp/research/external/<name>/` (the same convention as the corpus checkouts under
`/var/tmp/research/<mode>/`), pins what it fetched, and refuses to re-download an existing copy.

Why external data at all: our own ground truth (`src/test/data/diffs/`) was annotated by the
people who built CodeDiff. The introductory paper's threats-to-validity section names that risk.
An oracle built by someone else, for a different tool, on a different AST, is the answer to it.

## What is here

| Script | Fetches | Into | Size | License |
|---|---|---|---|---|
| `fetch_astdiff_oracle.sh` | Alikhanifard & Tsantalis' AST node-mapping benchmark, as checked into RefactoringMiner | `refactoringminer-astdiff/` | ~2.4 GB (sparse checkout, JSON) | MIT |
| `fetch_defects4j_metadata.sh` | Defects4J's per-project `active-bugs.csv` (buggy/fixed revision ids, bug-report URL) | `defects4j-meta/` | 60 KB | Defects4J (MIT) |
| `fetch_gumtree_simple_package.sh` | Falleri & Martinez' ICSE 2024 replication package: before/after file pairs for Defects4J, BugsInPy, gh-java, gh-python, plus their 100-case qualitative study | `gumtree-simple/` | 54 MB zip, ~300 MB extracted | CC-BY-4.0 |

The two go together. The oracle stores *mappings* (Eclipse JDT node type + character offsets, per
compilation unit) but not the *source files* they refer to; the Defects4J half of it is keyed
`Project/BugId/<path with / replaced by _>.json`, and the replication package's
`dataset/defects4j/{before,after}/Project/BugId/<same name>.java` is exactly the buggy/fixed pair
those offsets index into. Every one of the oracle's 996 Defects4J JSON files has its source pair
there (checked 2026-09-11). The refactoring half (`commits/<org>_<repo>/<sha>/`) needs the files
fetched from GitHub at that SHA - not wired up yet.

## The oracle, in one paragraph

Alikhanifard & Tsantalis, "A Novel Refactoring and Semantic Aware Abstract Syntax Tree
Differencing Tool and a Benchmark for Evaluating the Accuracy of Diff Tools", TOSEM 2025
(arXiv 2403.05939). 800 Defects4J bug fixes and (in the paper) 188 refactoring commits; the
checkout has grown to 296 commit directories since. Ground truth was built by running six tools
(RefactoringMiner 3.0, GumTree 3.0 greedy and simple, GumTree 2.1, IJM, MTDiff), inspecting every
diff by hand against six stated criteria, and taking the correct tool's output, a merge of several,
or a hand-corrected version; debatable cases went to a second author and then a group vote. Six
person-months. Java only. Each JSON file is a flat list of records:

```
{ "firstType": "ExpressionStatement", "secondType": "ExpressionStatement",
  "firstLabel": "...", "secondLabel": "...",
  "firstParentType": "Block", "secondParentType": "Block",
  "firstPos": 3293, "firstEndPos": 3364, "secondPos": 3293, "secondEndPos": 3364 }
```

Positions are Java `String` indices, i.e. **UTF-16 code units**, not bytes; the scorer converts.
The list is the *complete* mapping, identical subtrees included (one Joda-Time file carries
11,087 records), so precision and recall are only meaningful after the paper's exclusion rule:
drop every mapping nested under an unchanged program element (type, method, field, import...).

`defects4j/cases.json` lists 698 cases and `cases-problematic.json` a further 102; the scorer
runs both and carries the flag through to its CSV so the two populations can be reported apart.

## Scoring CodeDiff against it

```
cd research
make fetch-external            # both scripts, idempotent
make measure-astdiff-oracle    # -> data/comparison/astdiff_oracle_defects4j.csv + a summary
```

The scorer is `src/bin/benchmark_astdiff_oracle.rs` (feature `test-fixtures`, like every other
dev tool). What it does, and where the seams are, is documented at the top of that file; the short
version: a JDT node and a tree-sitter node are "the same node" when their byte spans are equal,
mappings are compared as sets of span pairs, and a CodeDiff mapping is only judged at all when the
oracle could have an opinion about it. The published reference numbers to land next to are
Table 12 of the paper (statement + sub-expression level, Defects4J):

| Tool | Precision | Recall |
|---|---|---|
| RefactoringMiner 3.0 | 99.7 | 99.3 |
| GumTree 3.0 simple | 98.4 | 97.8 |
| GumTree 3.0 greedy | 97.5 | 93.1 |

Those were computed on JDT trees with JDT's own notion of which nodes exist. Ours are computed on
tree-sitter trees through a span equality that cannot see every JDT node (the `METHOD_INVOCATION_RECEIVER`
kind of synthetic node has no tree-sitter counterpart; Javadoc is one `block_comment` to
tree-sitter and a subtree to JDT). The scorer reports how much of the oracle it could resolve, and
that number belongs next to any precision/recall quoted from it.

## First result (2026-09-11)

All 800 Defects4J cases, statement + sub-expression level: codediff **99.42% precision, 98.86%
recall**, perfect diff on 54.1% of cases; statement level 99.46 / 98.67, perfect 80.8%. Between
GumTree 3.0 simple and RefactoringMiner 3.0 on precision and recall at both granularities. The
per-file CSV, the exact oracle commit, the resolution rate (78% of records; 88% of those not
inside a comment) and the caveats that go with the comparison are in
`../data/comparison/PROVENANCE.md`.

## Promoting oracle cases into the corpus

`extract_defects4j_fixtures.py` (`make extract-defects4j-fixtures`, or `--cases Closure-157,...`)
turns the rows of the scorer's CSV where codediff disagrees with the oracle most into fixture
directories under `src/test/data/diffs/defects4j/`, the corpus's fifth dataset. Each gets the
buggy/fixed pair as `before.java.test`/`after.java.test`, a `README.md` in the provenance format
every other fixture carries (Defects4J's revision ids and bug report, via
`fetch_defects4j_metadata.sh`), and a `description.md` recording the disagreement that got it
picked. No mapping: the fixtures are unsolved until someone maps them in `human_solver`, whose
`o` picker lists them under the `defects4j` dataset like any other. The first 20 were promoted
on 2026-09-11. Our mapping is our own judgement; where it ends up disagreeing with the oracle,
the fixture's `description.md` is the place to say so.

## Other datasets looked at and not fetched (2026-09-11)

* **Fan et al., ICSE 2021** (Zenodo 4281091, CC-BY-4.0): 575 statements hand-labelled
  accurate/inaccurate per tool for GumTree, MTDiff and IJM, 12 experts on 200 of them.
  Statement-level, Java, ten Apache projects that must be cloned first. Smaller and coarser than
  the oracle above; worth adding only if a statement-level second opinion is wanted.
* **BDiff replication** (github.com/BDiff/BDiff-Evaluation-Experiment): 2997 Java/Python/XML cases,
  300 human 1-to-6 ratings, 3000 mutation cases with synthetic ground truth. "Academic research
  use only", no OSI license - do not redistribute anything derived from it.
* **GumTreeDiff/datasets, Megadiff, Diff-XYZ, CodeTracker**: file pairs or edit histories with no
  mapping ground truth. Speed corpora at best, and `data/samples/` already covers that.
