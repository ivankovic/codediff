Defects4J JacksonDatabind-53, promoted from the Alikhanifard & Tsantalis AST-diff oracle run of
2026-09-11 (`research/data/comparison/astdiff_oracle_defects4j.csv`) because codediff disagreed
with that oracle on 80 node pairs: 12 mappings the oracle does not have, 68 it has and
codediff does not (1 and 10 at statement level, out of 770 oracle
pairs scored). Listed by the oracle's authors as a problematic case. The human mapping here is our own; where it disagrees with the oracle, say
so in this file.
