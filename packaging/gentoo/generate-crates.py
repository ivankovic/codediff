#!/usr/bin/env python3
"""Regenerate the ``CRATES`` block of the Gentoo ebuild from Cargo.lock.

Gentoo's ``cargo.eclass`` fetches every crate in the dependency graph individually, so the ebuild
has to name all of them - 294 at the time of writing. ``pycargoebuild`` is the usual tool for this,
but it is not always installed, and the job is small enough to not need it: every registry crate in
Cargo.lock becomes one ``name@version`` line.

Only entries with a ``source`` key are emitted. The ``codediff`` package itself has none (it is the
workspace root, unpacked from the release tarball rather than fetched from crates.io), and neither
would any git or path dependency - none exist today, and if one is ever added it must be handled
explicitly in the ebuild rather than silently dropped into ``CRATES``, so this script fails loudly
instead of skipping it.

Usage:  python3 packaging/gentoo/generate-crates.py [--check]

Rewrites the ``CRATES="..."`` block of every ebuild in this directory in place. ``--check`` exits
non-zero instead of writing, for CI.
"""

import re
import sys
import tomllib
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
EBUILD_DIR = Path(__file__).resolve().parent / "dev-util" / "codediff"
CRATES_BLOCK = re.compile(r'^CRATES="\n.*?\n"$', re.MULTILINE | re.DOTALL)


def crates_from_lockfile(lockfile: Path) -> list[str]:
    data = tomllib.loads(lockfile.read_text())
    crates = []
    for package in data["package"]:
        source = package.get("source")
        if source is None:
            # The root crate. Anything else without a source is a path/git dependency that
            # cargo.eclass cannot fetch, and quietly omitting it would produce an ebuild that
            # fails to build only once someone runs it.
            if package["name"] != "codediff":
                raise SystemExit(
                    f"error: {package['name']} {package['version']} has no source - a path or git "
                    f"dependency needs explicit handling in the ebuild, not a silent drop"
                )
            continue
        if not source.startswith("registry+"):
            raise SystemExit(
                f"error: {package['name']} comes from {source!r}, not a registry - cargo.eclass's "
                f"CRATES mechanism only handles crates.io"
            )
        crates.append(f"{package['name']}@{package['version']}")
    return sorted(crates)


def main() -> int:
    check_only = "--check" in sys.argv[1:]
    crates = crates_from_lockfile(REPO_ROOT / "Cargo.lock")
    block = 'CRATES="\n' + "\n".join(f"\t{crate}" for crate in crates) + '\n"'

    ebuilds = sorted(EBUILD_DIR.glob("*.ebuild"))
    if not ebuilds:
        raise SystemExit(f"error: no ebuild found under {EBUILD_DIR}")

    stale = []
    for ebuild in ebuilds:
        text = ebuild.read_text()
        if not CRATES_BLOCK.search(text):
            raise SystemExit(f"error: {ebuild} has no CRATES=\"...\" block to replace")
        updated = CRATES_BLOCK.sub(lambda _: block, text, count=1)
        if updated == text:
            continue
        if check_only:
            stale.append(ebuild)
        else:
            ebuild.write_text(updated)
            print(f"updated {ebuild.relative_to(REPO_ROOT)} ({len(crates)} crates)")

    if stale:
        for ebuild in stale:
            print(f"error: {ebuild.relative_to(REPO_ROOT)} is out of date with Cargo.lock", file=sys.stderr)
        print("run: python3 packaging/gentoo/generate-crates.py", file=sys.stderr)
        return 1
    if check_only:
        print(f"CRATES up to date ({len(crates)} crates)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
