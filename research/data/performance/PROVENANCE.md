# Provenance

Measured against the PRE-2026-08-18 sampled corpus (byte-size strata: small/medium/large/xlarge in
the `size_bucket` column). `../samples/` has since been re-drawn under the LOC buckets
(`stats::sampling::LOC_BUCKETS`), so re-running `measure/benchmark_all_extended.sh` now measures a
DIFFERENT pair set than these files - do not mix rows across that boundary. `baselines/` snapshots
are pinned to whatever corpus was current at their date; that is their point.
