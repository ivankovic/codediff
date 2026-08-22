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

`nm_instances.md` is the authored counterpart: the changes whose true correspondence is N:M, which
neither this CSV nor `human_mapping.json` can represent. It is hand-curated from annotator
commentary on purpose - see its own header.
