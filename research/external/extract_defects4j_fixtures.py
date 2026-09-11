#!/usr/bin/env python3
"""Promote Defects4J cases from the AST-diff oracle run into `src/test/data/diffs/defects4j/`.

Reads `data/comparison/astdiff_oracle_defects4j.csv` (one row per compilation unit, written by
`benchmark_astdiff_oracle`), picks the cases where codediff disagrees most with the oracle, and
writes each as a fixture directory in the same shape `human_solver` expects for every other
dataset: `before.java.test`, `after.java.test`, a `README.md` whose provenance lines
`readme_provenance` (src/test/helper.rs) parses, and a `description.md` saying why the case was
picked. No `human_mapping.json` - that is what `human_solver` is for; until one is saved the
fixture is "unsolved" and every corpus reader skips it.

The source pair comes from the GumTree Simple replication package (the oracle's own offsets
index into those exact bytes - see README.md here), the revision ids and bug-report URL from
Defects4J's `active-bugs.csv` (fetch_defects4j_metadata.sh).

Usage, from research/:

    uv run ./external/extract_defects4j_fixtures.py --top 20
    uv run ./external/extract_defects4j_fixtures.py --cases Closure-157,Time-23

Selection is by `all_fp + all_fn` descending; `--top N` takes the N worst compilation units,
`--all` every one of the 996 (the paper's whole Defects4J benchmark, 800 cases). Existing fixture
directories are left untouched (a solved fixture must never be overwritten), and the script says
which it skipped.
"""

from __future__ import annotations

import argparse
import csv
import re
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
RESEARCH = HERE.parent
REPO = RESEARCH.parent

DEFAULT_CSV = RESEARCH / "data" / "comparison" / "astdiff_oracle_defects4j.csv"
DEFAULT_SOURCES = Path("/var/tmp/research/external/gumtree-simple/dataset/defects4j")
DEFAULT_META = Path("/var/tmp/research/external/defects4j-meta")
DEFAULT_INTO = REPO / "src" / "test" / "data" / "diffs" / "defects4j"

# Upstream repository of each Defects4J project. Defects4J bundles its own copies (Chart came
# from SourceForge SVN and has no upstream git at all; Closure predates the GitHub move), so the
# revision ids in active-bugs.csv are not guaranteed to exist at these URLs - the README says so.
UPSTREAM = {
    "Chart": "https://github.com/jfree/jfreechart.git",
    "Cli": "https://github.com/apache/commons-cli.git",
    "Closure": "https://github.com/google/closure-compiler.git",
    "Codec": "https://github.com/apache/commons-codec.git",
    "Collections": "https://github.com/apache/commons-collections.git",
    "Compress": "https://github.com/apache/commons-compress.git",
    "Csv": "https://github.com/apache/commons-csv.git",
    "Gson": "https://github.com/google/gson.git",
    "JacksonCore": "https://github.com/FasterXML/jackson-core.git",
    "JacksonDatabind": "https://github.com/FasterXML/jackson-databind.git",
    "JacksonXml": "https://github.com/FasterXML/jackson-dataformat-xml.git",
    "Jsoup": "https://github.com/jhy/jsoup.git",
    "JxPath": "https://github.com/apache/commons-jxpath.git",
    "Lang": "https://github.com/apache/commons-lang.git",
    "Math": "https://github.com/apache/commons-math.git",
    "Mockito": "https://github.com/mockito/mockito.git",
    "Time": "https://github.com/JodaOrg/joda-time.git",
}


def slug(url: str) -> str:
    """`owner-repo.git`, the clone-directory form every other fixture README records."""
    return "-".join(url.rstrip("/").split("/")[-2:])


def read_bug_table(meta: Path, project: str) -> dict[str, dict[str, str]]:
    """Every bug Defects4J has ever shipped for `project`, active and deprecated alike.

    The oracle was built on an older Defects4J than today's; bugs it has since deprecated (a
    row in `deprecated-bugs.csv` instead of `active-bugs.csv`, with the same columns plus a
    reason) are still cases in the benchmark and still have their sources in the replication
    package, so they are promoted like any other, with the deprecation noted in the README.
    """
    table: dict[str, dict[str, str]] = {}
    for suffix, deprecated in ((".csv", False), (".deprecated.csv", True)):
        path = meta / f"{project}{suffix}"
        if not path.is_file():
            sys.exit(f"{path} missing - run external/fetch_defects4j_metadata.sh")
        with path.open(newline="") as handle:
            for row in csv.DictReader(handle):
                row["deprecated"] = deprecated
                table.setdefault(row["bug.id"], row)
    return table


def fixture_name(project: str, bug: str, flat_path: str, segments: int = 1) -> str:
    """`java-defects4j-closure-157-codegenerator`: the dataset, the case, the file's base name.

    Fixture names double as Rust module names once `human_solver` writes the stub test (dashes
    become underscores), so only `[a-z0-9-]` survives. `segments` > 1 pulls that many trailing
    path segments in (`...-parser-tag`) - only used when two files of one case share a base
    name, which happens once in the 996 (Math-4 has a `SubLine.java` in both `twod` and `threed`).
    """
    parts = flat_path.removesuffix(".java").split("_")
    stem = "-".join(parts[-segments:]).lower()
    stem = re.sub(r"[^a-z0-9]+", "-", stem).strip("-")
    return f"java-defects4j-{project.lower()}-{bug}-{stem}"


def unique_names(rows: list[dict[str, str]]) -> dict[str, str]:
    """`(case, file)` -> fixture name, widened with path segments until every name is unique."""
    names: dict[str, str] = {}
    by_name: dict[str, list[tuple[str, str]]] = {}
    for row in rows:
        project, bug = row["case"].split("-", 1)
        flat = row["file"].removesuffix(".json") + ".java"
        by_name.setdefault(fixture_name(project, bug, flat), []).append((row["case"], flat))
    for name, members in by_name.items():
        if len(members) == 1:
            names[members[0]] = name
            continue
        segments = 2
        while True:
            widened = {m: fixture_name(*m[0].split("-", 1), m[1], segments) for m in members}
            if len(set(widened.values())) == len(members):
                names.update(widened)
                break
            segments += 1
    return names


def real_path(flat_path: str) -> str:
    """Best-effort undo of the `/` -> `_` flattening, for the README's File line.

    Java package directories never contain `_`, but a file name can (`Foo_Bar.java`), so only
    the segments before the basename are unflattened; the basename is kept as it was.
    """
    directory, _, basename = flat_path.rpartition("_")
    directory = directory.replace("_", "/")
    # The flattening also ate the `/` before the basename.
    return f"{directory}/{basename}" if directory else basename


def deprecation_note(meta: dict[str, str]) -> str:
    if not meta.get("deprecated"):
        return ""
    reason = meta.get("deprecated.reason") or meta.get("reason") or "no reason recorded"
    version = meta.get("deprecated.version") or meta.get("version") or "?"
    return (
        f"\nDefects4J has since deprecated this bug (in version {version}: {reason}); the AST-diff"
        " oracle and the replication package predate that and still carry it.\n"
    )


def render_readme(project: str, bug: str, flat_path: str, meta: dict[str, str]) -> str:
    upstream = UPSTREAM[project]
    return f"""# Sample provenance

- **Repository:** {upstream} (`{slug(upstream)}`)
- **Commit:** `{meta["revision.id.fixed"]}`
- **File:** `{real_path(flat_path)}`
- **Research dataset:** defects4j

This fixture is Defects4J bug **{project}-{bug}** (https://github.com/rjust/defects4j): `before.java.test`
is the buggy revision (`{meta["revision.id.buggy"]}`) and `after.java.test` the fixed revision
(`{meta["revision.id.fixed"]}`) of the file above, byte for byte as shipped in the GumTree Simple
replication package (Falleri & Martinez, ICSE 2024, https://doi.org/10.5281/zenodo.10474674),
which is also the exact text the Alikhanifard & Tsantalis AST-diff oracle's offsets index into.
The revision ids are Defects4J's own, from the repositories it bundles, and are not guaranteed
to resolve in the upstream repository's history. Original bug report: {meta["report.url"]}
{deprecation_note(meta)}
This content is **not** part of codediff's own codebase and is **not** covered by codediff's own
AGPL-3.0 license - it remains under the license of the project it came from.

## License

The upstream project's license, as it read at the revision above, in the repository listed at the
top of this file.
"""


def render_description(row: dict[str, str], project: str, bug: str) -> str:
    fp, fn = int(row["all_fp"]), int(row["all_fn"])
    sfp, sfn = int(row["statement_fp"]), int(row["statement_fn"])
    flag = (
        " Listed by the oracle's authors as a problematic case."
        if row["problematic"] == "true"
        else ""
    )
    scored = row["all_oracle_scored"]
    if fp + fn == 0:
        verdict = (
            f"codediff agreed with that oracle on every one of the {scored} node pairs it scored"
        )
    else:
        verdict = (
            f"codediff disagreed with that oracle on {fp + fn} node pairs: {fp} mappings the oracle "
            f"does not have, {fn} it has and codediff does not ({sfp} and {sfn} at statement level, "
            f"out of {scored} oracle pairs scored)"
        )
    return f"""Defects4J {project}-{bug}, one compilation unit of the Alikhanifard & Tsantalis AST-diff
benchmark (TOSEM 2025), promoted from the oracle run of 2026-09-11
(`research/data/comparison/astdiff_oracle_defects4j.csv`), where {verdict}.{flag} The human
mapping here is our own; where it disagrees with the oracle, say so in this file.
"""


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--csv", type=Path, default=DEFAULT_CSV)
    parser.add_argument("--sources", type=Path, default=DEFAULT_SOURCES)
    parser.add_argument("--meta", type=Path, default=DEFAULT_META)
    parser.add_argument("--into", type=Path, default=DEFAULT_INTO)
    parser.add_argument("--top", type=int, default=0, help="promote the N worst compilation units")
    parser.add_argument("--all", action="store_true", help="promote every compilation unit")
    parser.add_argument("--cases", default="", help="comma-separated Project-BugId list to promote")
    args = parser.parse_args()

    with args.csv.open(newline="") as handle:
        rows = list(csv.DictReader(handle))
    rows.sort(key=lambda r: -(int(r["all_fp"]) + int(r["all_fn"])))

    wanted = {c.strip() for c in args.cases.split(",") if c.strip()}
    if wanted:
        rows = [r for r in rows if r["case"] in wanted]
        missing = wanted - {r["case"] for r in rows}
        if missing:
            sys.exit(f"not in {args.csv}: {', '.join(sorted(missing))}")
    elif args.top > 0:
        rows = rows[: args.top]
    elif not args.all:
        sys.exit("give --top N, --all, or --cases a,b,c")
    names = unique_names(rows)

    args.into.mkdir(parents=True, exist_ok=True)
    tables: dict[str, dict[str, dict[str, str]]] = {}
    written, skipped = [], []
    for row in rows:
        project, bug = row["case"].split("-", 1)
        flat_path = row["file"].removesuffix(".json") + ".java"
        name = names[(row["case"], flat_path)]
        target = args.into / name
        if target.exists():
            skipped.append(name)
            continue
        before = args.sources / "before" / project / bug / flat_path
        after = args.sources / "after" / project / bug / flat_path
        if not (before.is_file() and after.is_file()):
            sys.exit(f"source pair missing for {row['case']} {flat_path} under {args.sources}")
        table = tables.setdefault(project, read_bug_table(args.meta, project))
        meta = table.get(bug)
        if meta is None:
            sys.exit(f"{project}-{bug} not in Defects4J's active-bugs.csv")

        target.mkdir()
        (target / "before.java.test").write_bytes(before.read_bytes())
        (target / "after.java.test").write_bytes(after.read_bytes())
        (target / "README.md").write_text(render_readme(project, bug, flat_path, meta))
        (target / "description.md").write_text(render_description(row, project, bug))
        written.append(name)

    for name in written:
        print(f"wrote   {name}")
    for name in skipped:
        print(f"skipped {name} (already exists)")
    print(f"{len(written)} written, {len(skipped)} skipped, into {args.into}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
