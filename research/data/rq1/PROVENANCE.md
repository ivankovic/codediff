# Provenance

`apted_only_group*.csv` currently hold measurements against the PRE-2026-08-18 corpus (the one
archived in `archive_pre_resample_2026-08-18/` matches them). `../samples/` has since been re-drawn
under the LOC buckets with 22 languages (was 19), so `make rq1` now measures a different, larger
pair set - see `partial_resample_2026-08-18_INCOMPLETE/README.md` for the stopped first attempt.
Until a full re-measurement lands, per-bucket rates from these files and corpus counts from
`../samples/` must not be quoted side by side as one dataset.
