# Partial RQ1 re-measurement (INCOMPLETE - do not report from this)

Stopped by request partway through group 1 of 4, on 2026-08-20. Measured against the re-sampled,
LOC-stratified corpus **as it stood before the 2026-08-20 `--depth=50` re-fetch**, which is what
makes it worthless as a result rather than merely partial: roughly 41% of the pairs it attempted
could not be read at all, because the sample pointed at commits no local clone still had. See
`../PROVENANCE.md` for that failure and how it was found.

`../apted_only_group1.csv` was restored from `../archive_pre_resample_2026-08-18/` when this run was
stopped, so `make rq1-report` read four internally consistent old-corpus files rather than blending
one partial new-corpus file with three old-corpus ones. That restore has since been superseded: all
four group files now hold the completed 2026-08-21 measurement.

Kept only as a record of the run that exposed the stale-sample problem. The measurement it was
attempting was redone properly on 2026-08-21 by `measure/overnight_rq1_refresh.sh`, against a
corpus re-fetched at `--depth=50` and a sample verified to resolve completely.
