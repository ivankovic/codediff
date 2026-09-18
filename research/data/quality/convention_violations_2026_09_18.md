# Cross-fixture painting conventions, 2026-09-18

The seventeen rules in `invariants.rs` are all *intra*-fixture: they ask whether one fixture's
mapping and paintings agree with each other. None compares two fixtures, so a pair whose paintings
answer the same question differently is invisible to all of them — and a corpus that disagrees with
itself across fixtures grades the renderer against a coin flip however self-consistent each fixture
is. This is that missing axis. Regenerate with:

    cargo test --release --lib --features test-fixtures cross_fixture_convention_census -- --ignored --nocapture

Written to `convention_census.csv`: one row per exception, with its own class's `clean` count
beside it as the denominator.

## Method

The population is the one place two fixtures are comparable: a leaf the human's **own tree
mapping** pairs with a leaf reading the same text. No matcher is involved and no judgement about
what the change *is* — the human has already said this text survived. What is left is where it
ended up (geometry) and whether the painting colours it (convention).

Four facts, all about the files rather than about any rule in `diff::text` — a class defined by the
renderer's own predicates could only re-discover the renderer:

* `row_moved`, `column_moved` — the leaf's start against its partner's.
* `row_edited` — whether its whole row reads differently on the two sides.
* `drift_matches_neighbours` — whether its row delta is the one its immediate neighbours also
  carry. This is what separates the two things `row_moved` conflates: text pushed down by an
  insertion above moves by the same amount as everything around it; a block that genuinely
  relocated does not.

**3,276,710 leaves, 4,736 exceptions (0.14%).**

## The baseline, which is what makes the rest readable

| preset | class | fixtures painting none | painting some | leaves |
| --- | --- | ---: | ---: | ---: |
| Minimal | in place, row unchanged | 322 | **1** | 837,528 |
| Full | in place, row unchanged | 310 | **3** | 825,064 |

A leaf that did not move, on a row that did not change, is unpainted in 632 of 636 fixture-preset
pairs across 1.66M leaves. That near-unanimity is what licenses calling the deviations errors
rather than opinions.

**All four deviations, 38 leaves:**

| fixture | preset | leaves |
| --- | --- | ---: |
| `java-defects4j-lang-17-charsequencetranslator` | Full | 32 |
| `java-defects4j-cli-12-gnuparser` | Minimal | 2 |
| `java-defects4j-cli-12-gnuparser` | Full | 2 |
| `java-defects4j-cli-15-writeablecommandlineimpl` | Full | 2 |

Every one is painted `Move`. Nothing about the leaf or its row changed on either side, so there is
nothing for a `Move` to describe.

## The contested convention: is a pure displacement a `Move`?

A leaf that kept its column and its row's entire content, moved down the file by the same amount as
its neighbours — pushed by an insertion somewhere above it. **235 Full fixtures and 255 Minimal
fixtures paint none of these. 17 paint some.**

Numbers below are `painted/clean` **within the same class, in the same fixture**, so a large right
number is the fixture disagreeing with itself and not only with the corpus.

### Contradicts `Minimal` as well (9)

`Minimal` is specified to paint as few bytes as it can and to prefer not to paint pure identical
moves, and 255 fixtures do exactly that. These nine do not.

| Minimal | Full | fixture |
| ---: | ---: | --- |
| 26/1722 | 26/1722 | `java-defects4j-chart-22-keyedobjects2d` |
| 24/100 | 24/100 | `java-defects4j-jsoup-52-xmldeclaration` |
| 22/54 | 22/54 | `java-defects4j-cli-14-groupimpl` |
| 12/22 | 12/22 | `java-defects4j-mockito-11-delegatingmethod` |
| 10/50 | 10/50 | `java-defects4j-lang-28-numericentityunescaper` |
| 8/792 | 8/792 | `java-defects4j-jacksondatabind-94-subtypevalidator` |
| 6/0 | 6/0 | `rust-multi-map-duplicate-calls` |
| 5/0 | 5/0 | `c-neovim-neovim-small-change` |
| 2/274 | 2/274 | `go-grafana-grafana-real-small-change-with-a-move` |

Seven of the nine carry a clean count far larger than their painted one — `jacksondatabind-94`
paints 8 of 800 and `chart-22` 26 of 1,748. Those are not a convention, because a convention
applied 1% of the time is not being applied; they are seven fixtures contradicting themselves, and
no cross-fixture comparison is needed to condemn them. Only `rust-multi-map-duplicate-calls` and
`c-neovim-neovim-small-change` paint *every* such leaf, so only those two are a considered choice
to disagree with.

### `Full` only (8)

| Full | fixture |
| ---: | --- |
| 104/330 | `rust-next-font-imports-generator` |
| 64/33660 | `rust-small-addition-with-reuse-of-binary-expressions` |
| 56/468 | `java-defects4j-lang-17-charsequencetranslator` |
| 40/2 | `rust-add-if` |
| 22/294 | `java-defects4j-mockito-15-finalmockcandidatefilter` |
| 4/1318 | `java-defects4j-cli-15-writeablecommandlineimpl` |
| 2/2 | `c-genymobile-scrcpy-big-change` |
| 2/572 | `java-defects4j-jxpath-18-attributecontext` |

This half is the documented preset split — `rust-add-if`'s own ground truth is on record wanting
this shape painted `Move` under `Full` and unpainted under `Minimal`, and it is the only fixture
here that applies it nearly throughout (40 of 42). The other seven apply it to between 0.2% and 24%
of their eligible leaves while 235 Full fixtures apply it to none, so the split is real but these
seven are not evidence for it.

## What the census got right on its own

`ruby-mastodon-mastodon-move`, `ruby-mastodon-mastodon-rare-example-of-true-move`,
`cpp-godot-small-bugfix` and `php-nextcloud-server-real-small-change` all paint displaced-looking
leaves `Move` and are **not** in either table: every one of their painted leaves has a row delta
its neighbours do not share, which is a genuine relocation. Two of them say so in their own names.
`drift_matches_neighbours` is what keeps them out, and they are the check on it.

## Suggested order

1. **The 38 in-place leaves** (4 fixture-presets). Unpaintable by any reading; no decision needed.
2. **The 7 self-contradicting displacement fixtures** — `chart-22`, `jsoup-52`, `cli-14-groupimpl`,
   `mockito-11`, `lang-28`, `jacksondatabind-94`, `grafana` — plus the five `Full`-side ones with
   the same shape. Each needs its minority leaves unpainted to match its own majority.
3. **`rust-multi-map-duplicate-calls` and `c-neovim-neovim-small-change`** — a real decision, and
   the only one here. Two fixtures against 255.
4. **The `Full` displacement convention itself** — `rust-add-if` is on record for it, 235 fixtures
   are against it, and nothing in the corpus states which is meant. Worth settling in writing
   before either side is edited.
