# Provenance

Measured against the human-authored ground-truth fixtures in `src/test/data/diffs/` (NOT the
sampled corpus in `../samples/` - quality is scored on hand-verified mappings, so re-sampling the
corpus does not stale these files). `quality_baseline.txt` is updated only via
`make update-quality-baseline` at the repository root.

`human_mapping_analysis.csv` also defines *which* fixtures are in scope for
`analysis/ambiguity_report.py` (the paper's RQ3): that script reads the `human_mapping.json` files
directly but restricts itself to the names listed here, so every Section 5 number describes one
corpus state. Refresh order after adding fixtures: `analyze_human_mappings --csv`, then
`make ambiguity-report`.

## Refresh of 2026-09-16

Both `optimal_solutions_benchmark.csv` and `human_mapping_analysis.csv` were re-run, in that
order, against the corpus at commit `4ad53c4f` plus the working tree. What the run covers:

| | fixtures | with a `human_mapping.json` |
|---|---|---|
| `handmade` | 62 | 61 (60 scored - one mapping is empty) |
| `small` (Curated) | 220 | 220 |
| `full` (Full) | 232 | 232 |
| `stratified` | 491 | 491 |
| `defects4j` | 996 | 113 |
| **total** | **2001** | **1117** |

`optimal_solutions_benchmark.csv` therefore has 2001 rows, 1116 of them with ground truth, against
951 and 949 before, and `human_mapping_analysis.csv` has 1117 rows against 836. The paper's scope - `_common.PAPER_DATASETS`, which is now these four minus
`handmade` - is **1056** fixtures, against 775 at the 2026-09-09 state.

**`defects4j` became a reported dataset in this pass** (see `_common.PAPER_DATASETS` and
`research/external/README.md`). It is scored beside the other three and folded into every pooled
total. Only its 113 solved units are ever scored; the remaining 883 directories carry no mapping
and are invisible to every report. Which 113 are solved is how far annotation has reached, not a
draw - do not read a Defects4J figure as an estimate over Defects4J.

The run also picked up the ground-truth repairs and invariant corrections of commits `523ffb23`
and `4ad53c4f`, so the numbers move slightly even on fixtures that were already in scope: over the
same 775 fixtures as 2026-09-09, `NodesMatched` went 5,475,305 -> 5,475,322.

`nm_instances.md` is the authored counterpart: the changes whose true correspondence is N:M, which
neither this CSV nor `human_mapping.json` can represent. It is hand-curated from annotator
commentary on purpose - see its own header.
