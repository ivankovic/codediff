# Partial RQ1 re-measurement (INCOMPLETE - do not report from this)

Stopped by request partway through group 1 of 4, on 2026-08-18. Contains 2,658 of that group's
4,888 pairs (CSS, Python, XML, and nothing from the remaining three groups at all), measured
against the re-sampled, LOC-stratified corpus.

Not usable as an RQ1 result on its own: it covers three languages out of 22 and less than a
seventh of the corpus, and its language mix is whatever group 1 happened to reach alphabetically -
not a sample of anything. Kept only so the run does not have to restart from zero if it is
resumed.

`research/data/rq1/apted_only_group1.csv` was restored from `../archive_pre_resample_2026-08-18/`
so that `make rq1-report` reads four internally consistent files from the OLD corpus rather than
silently blending this partial new-corpus file with three old-corpus ones.

To resume: re-run `make measure-rq1` from the top (it re-measures every group; there is no per-group
resume), or run the four `apted_only_benchmark` invocations in that target individually.
