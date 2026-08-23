# This file is part of the CodeDiff code diffing tool.
#
# Copyright (C) 2026 Marko Ivankovic
#
# This program is free software: you can redistribute it and/or modify
# it under the terms of the GNU Affero General Public License as published
# by the Free Software Foundation, either version 3 of the License, or
# (at your option) any later version.
#
# This program is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
# GNU Affero General Public License for more details.
#
# You should have received a copy of the GNU Affero General Public License
# along with this program.  If not, see <https://www.gnu.org/licenses/>.
"""BDiff runner with two modes: one pair as JSON on stdout, or a timed batch over many pairs.

BDiff ships a CLI (`python -m bdiff a b`) but it *discards* the return value of `bdiff.bdiff()` and
prints nothing at all - confirmed live, 2026-08-23: exit code 0, empty stdout, empty stderr. The
edit script is only available from the library API, so this driver exists to expose it. Embedded
into `benchmark_other` via `include_str!` and written to a temp file at run time, the same way
`generate_mapping_site` embeds its own JavaScript, so there is no separate file to keep in sync
with the binary that runs it.

BDiff internally shells out to `git diff --no-index --diff-algorithm=... --unified=0 --numstat`
for raw change detection, which is why `benchmark_other` runs this driver with GIT_CONFIG_GLOBAL
and GIT_CONFIG_SYSTEM pointed at /dev/null. Without that, a user-level `diff.external` (this
project's own README recommends setting exactly that, to codediff itself) replaces git's diff
output, BDiff parses no `@@` headers, and it returns a **0-entry edit script with exit code 0** -
a silently perfect-looking score rather than an error. See data/comparison/PROVENANCE.md.

=== Batch mode ===

`bdiff_driver.py --batch` reads line-delimited JSON requests `{"id", "before", "after"}` from
stdin and writes one `{"id", "ms"}` response line per request, timing *only* the `bdiff.bdiff()`
call. This mirrors research/drivers/gumtree-batch for the same reason it exists: importing bdiff
pulls in numpy, scipy and rapidfuzz, which costs ~394 ms against a ~12 ms bare interpreter
(measured 2026-08-23), so a per-invocation wall-clock number is ~97% import overhead and says
almost nothing about the algorithm. `benchmark_other` reports both - `bdiff_ms` per process and
`bdiff_warm_ms` from this batch - exactly as it already does for GumTree's cold and warm JVM.
"""

import json
import sys
import time

import bdiff

if "--batch" in sys.argv[1:]:
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        request = json.loads(line)
        start = time.perf_counter()
        try:
            bdiff.bdiff(request["before"], request["after"])
        except Exception as exc:  # noqa: BLE001 - one bad pair must not kill the batch
            print(json.dumps({"id": request["id"], "error": str(exc)}), flush=True)
            continue
        elapsed_ms = (time.perf_counter() - start) * 1000.0
        print(json.dumps({"id": request["id"], "ms": elapsed_ms}), flush=True)
    raise SystemExit(0)

if len(sys.argv) != 3:
    print("usage: bdiff_driver.py <before> <after> | --batch", file=sys.stderr)
    raise SystemExit(2)

script = bdiff.bdiff(sys.argv[1], sys.argv[2])
json.dump(script, sys.stdout, default=str)
