#!/usr/bin/env python3
#
#  This file is part of the CodeDiff code diffing tool.
#
#  Copyright (C) 2026 Marko Ivankovic
#
#  This program is free software: you can redistribute it and/or modify
#  it under the terms of the GNU Affero General Public License as published
#  by the Free Software Foundation, either version 3 of the License, or
#  (at your option) any later version.
#
#  This program is distributed in the hope that it will be useful,
#  but WITHOUT ANY WARRANTY; without even the implied warranty of
#  MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
#  GNU Affero General Public License for more details.
#
#  You should have received a copy of the GNU Affero General Public License
#  along with this program.  If not, see <https://www.gnu.org/licenses/>.
"""Checks that every packaging recipe names the version `Cargo.toml` does.

`Cargo.toml` is the source of truth: the binary reports `CARGO_PKG_VERSION`, `make deploy-github`
tags `v<that>`, and `release.yml` builds whatever the tag names. Three recipes under packaging/
repeat the number by hand, and nothing else compares them:

* `packaging/aur/PKGBUILD` - `pkgver=`
* `packaging/gentoo/dev-util/codediff/codediff-<version>.ebuild` - the file name
* `packaging/nix/package.nix` - the `version ?` fallback

A recipe left on the previous version fetches the previous tag's tarball and builds the previous
release under the new number, which is worse than failing. Run by `make check-versions`, which
`deploy-checks` and CI's python job both call.

Usage:  python3 scripts/check_version_sync.py
"""

import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
CARGO_TOML = REPO_ROOT / "Cargo.toml"
PKGBUILD = REPO_ROOT / "packaging" / "aur" / "PKGBUILD"
EBUILD_DIR = REPO_ROOT / "packaging" / "gentoo" / "dev-util" / "codediff"
PACKAGE_NIX = REPO_ROOT / "packaging" / "nix" / "package.nix"


def first_match(pattern: str, text: str, what: str) -> str:
    match = re.search(pattern, text, re.MULTILINE)
    if match is None:
        sys.exit(f"check_version_sync: could not find {what}")
    return match.group(1)


def cargo_version() -> str:
    """The `[package]` version: the first `version = "..."` line, which precedes every table."""
    return first_match(r'^version = "([^"]+)"', CARGO_TOML.read_text(), "version in Cargo.toml")


def recipe_versions() -> dict[str, str]:
    """Each recipe's version, keyed by a path relative to the repository root."""
    ebuilds = sorted(EBUILD_DIR.glob("codediff-*.ebuild"))
    if len(ebuilds) != 1:
        sys.exit(
            f"check_version_sync: expected exactly one ebuild in {EBUILD_DIR.relative_to(REPO_ROOT)},"
            f" found {len(ebuilds)}"
        )
    ebuild = ebuilds[0]
    return {
        str(PKGBUILD.relative_to(REPO_ROOT)): first_match(
            r"^pkgver=(\S+)", PKGBUILD.read_text(), "pkgver in PKGBUILD"
        ),
        str(ebuild.relative_to(REPO_ROOT)): ebuild.name.removeprefix("codediff-").removesuffix(
            ".ebuild"
        ),
        str(PACKAGE_NIX.relative_to(REPO_ROOT)): first_match(
            r'^\s*version \? "([^"]+)"', PACKAGE_NIX.read_text(), "version fallback in package.nix"
        ),
    }


def main() -> int:
    expected = cargo_version()
    stale = {path: found for path, found in recipe_versions().items() if found != expected}
    if stale:
        print(f"check_version_sync: Cargo.toml says {expected}, but:", file=sys.stderr)
        for path, found in stale.items():
            print(f"  {path}: {found}", file=sys.stderr)
        print("See the release checklist in packaging/README.md for what to bump.", file=sys.stderr)
        return 1
    print(f"check_version_sync: every recipe names {expected}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
