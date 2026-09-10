#!/usr/bin/env python3
"""Regenerate the ``CRATES`` block of the Gentoo ebuild from Cargo.lock.

Lives here rather than beside the ebuild so ruff covers it: CI lints research/, scripts/ and
assets/ only.

Gentoo's ``cargo.eclass`` fetches every crate in the dependency graph individually, so the ebuild
has to name all of them - 294 at the time of writing. ``pycargoebuild`` is the usual tool for this,
but it is not always installed, and the job is small enough to not need it: every registry crate in
Cargo.lock becomes one ``name@version`` line.

Only entries with a ``source`` key are emitted. The ``codediff`` package itself has none (it is the
workspace root, unpacked from the release tarball rather than fetched from crates.io), and neither
would any git or path dependency - none exist today, and if one is ever added it must be handled
explicitly in the ebuild rather than silently dropped into ``CRATES``, so this script fails loudly
instead of skipping it.

Usage:  python3 scripts/generate_gentoo_crates.py [--check]

Rewrites the ``CRATES="..."`` block of every ebuild under packaging/gentoo/ in place. ``--check`` exits
non-zero instead of writing, for CI.
"""

import re
import sys
import tomllib
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[1]
EBUILD_DIR = REPO_ROOT / "packaging" / "gentoo" / "dev-util" / "codediff"
# Deliberately NOT `.*?` with DOTALL: a lazy dot-star still crosses newlines, so it runs past the
# end of this block to the *next* line consisting of a lone closing quote - SRC_URI's, here - and
# silently deletes `inherit`, DESCRIPTION, HOMEPAGE and SRC_URI along the way. (It did exactly
# that once.) Matching only tab-indented crate lines cannot overrun, and an empty block still
# matches because the repetition allows zero lines.
CRATES_BLOCK = re.compile(r'^CRATES="\n(?:\t[^\n]*\n)*"$', re.MULTILINE)


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


# Everything outside the CRATES block that must survive a substitution. The regex above is
# anchored and shape-restricted so it cannot overrun today, but a destroyed ebuild is perfectly
# self-consistent - `--check` reported "up to date" on a headerless one - so nothing else in this
# script can notice the damage. This can, and it costs one pass over a 374-line file.
REQUIRED_AFTER_SUBSTITUTION = (
    "inherit ",
    "DESCRIPTION=",
    "HOMEPAGE=",
    "SRC_URI=",
    "LICENSE=",
    "src_install()",
)


def _assert_intact(ebuild: Path, updated: str) -> None:
    missing = [token for token in REQUIRED_AFTER_SUBSTITUTION if token not in updated]
    if missing:
        raise SystemExit(
            f"error: substituting CRATES into {ebuild.name} removed {', '.join(missing)} - "
            f"refusing to write. The CRATES regex has overrun its block; fix it before rerunning."
        )


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
            raise SystemExit(f'error: {ebuild} has no CRATES="..." block to replace')
        updated = CRATES_BLOCK.sub(lambda _: block, text, count=1)
        _assert_intact(ebuild, updated)
        if updated == text:
            continue
        if check_only:
            stale.append(ebuild)
        else:
            ebuild.write_text(updated)
            print(f"updated {ebuild.relative_to(REPO_ROOT)} ({len(crates)} crates)")

    if stale:
        for ebuild in stale:
            print(
                f"error: {ebuild.relative_to(REPO_ROOT)} is out of date with Cargo.lock",
                file=sys.stderr,
            )
        print("run: python3 scripts/generate_gentoo_crates.py", file=sys.stderr)
        return 1
    if check_only:
        print(f"CRATES up to date ({len(crates)} crates)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
