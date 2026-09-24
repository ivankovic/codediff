#!/usr/bin/env python3
"""Regenerate the Gentoo ebuild's ``CRATES`` block, and the Manifest's crate digests, from Cargo.lock.

Lives here rather than beside the ebuild so ruff covers it: CI lints research/, scripts/ and
assets/ only.

Gentoo's ``cargo.eclass`` fetches every crate in the dependency graph individually, so the ebuild
has to name all of them - 265 at the time of writing. ``pycargoebuild`` is the usual tool for this,
but it is not always installed, and the job is small enough to not need it: every registry crate in
Cargo.lock becomes one ``name@version`` line.

Only entries with a ``source`` key are emitted. The ``codediff`` package itself has none (it is the
workspace root, unpacked from the release tarball rather than fetched from crates.io), and neither
would any git or path dependency - none exist today, and if one is ever added it must be handled
explicitly in the ebuild rather than silently dropped into ``CRATES``, so this script fails loudly
instead of skipping it.

Usage:  python3 scripts/generate_gentoo_crates.py [--check] [--manifest]

Rewrites the ``CRATES="..."`` block of every ebuild under packaging/gentoo/ in place. ``--check``
exits non-zero instead of writing, for CI.

**The Manifest is checked too, and that is a separate failure from the ebuild's.** Gentoo fetches
each crate against the Manifest's digests, and until 2026-09-18 nothing compared it to anything:
``--check`` passed on a Manifest whose ``CRATES`` list was current while **65 of its crate
digests named older versions and one crate had no line at all**, because the two files had drifted
apart over dependency bumps that only ever touched the ebuild. ``--check`` now also asserts that
the set of ``name-version.crate`` DIST lines is exactly Cargo.lock's - the drift that actually
happened, caught without needing a single byte of any crate.

It does not check the *digests*, because BLAKE2B and SHA512 cannot be derived from the sha256
Cargo.lock records; that needs the files. ``--manifest`` rebuilds those lines from ``.crate`` files
in the local cargo registry cache, verifying each one's sha256 against Cargo.lock before hashing
it, and downloading whatever the cache is missing. The ``.tar.gz`` line is left alone either way:
it hashes the GitHub *tag* tarball, which does not exist until a release is tagged, so it is a
post-release step by construction (see packaging/README.md).
"""

import hashlib
import re
import sys
import tomllib
import urllib.request
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


MANIFEST = EBUILD_DIR / "Manifest"
# Where cargo keeps the `.crate` files it has already fetched. One directory per registry.
CARGO_CACHE = Path.home() / ".cargo" / "registry" / "cache"
CRATES_IO_DOWNLOAD = "https://static.crates.io/crates/{name}/{name}-{version}.crate"


def packages_from_lockfile(lockfile: Path) -> list[tuple[str, str, str]]:
    """``(name, version, sha256)`` for every registry crate, in Manifest order."""
    data = tomllib.loads(lockfile.read_text())
    packages = []
    for package in data["package"]:
        if package.get("source") is None:
            continue
        checksum = package.get("checksum")
        if checksum is None:
            raise SystemExit(
                f"error: {package['name']} {package['version']} has no checksum in Cargo.lock - "
                f"there is nothing to verify a downloaded crate against, so it is not hashed"
            )
        packages.append((package["name"], package["version"], checksum))
    return sorted(packages)


def crate_bytes(name: str, version: str, sha256: str) -> bytes:
    """The crate's own bytes, from the cargo cache or crates.io, verified against Cargo.lock.

    The verification is the point, and it is why this can use a cache it does not control: a file
    whose sha256 is the one Cargo.lock already records is the file cargo itself would build, and
    one whose sha256 is anything else is not hashed into a Manifest under any circumstances.
    """
    filename = f"{name}-{version}.crate"
    for cached in CARGO_CACHE.glob(f"*/{filename}"):
        data = cached.read_bytes()
        if hashlib.sha256(data).hexdigest() == sha256:
            return data
    url = CRATES_IO_DOWNLOAD.format(name=name, version=version)
    with urllib.request.urlopen(url) as response:  # noqa: S310 - fixed https host
        data = response.read()
    actual = hashlib.sha256(data).hexdigest()
    if actual != sha256:
        raise SystemExit(
            f"error: {filename} downloaded from crates.io hashes to {actual}, but Cargo.lock "
            f"records {sha256} - refusing to write a digest for it"
        )
    return data


def manifest_dist_line(name: str, version: str, data: bytes) -> str:
    return (
        f"DIST {name}-{version}.crate {len(data)} "
        f"BLAKE2B {hashlib.blake2b(data).hexdigest()} "
        f"SHA512 {hashlib.sha512(data).hexdigest()}"
    )


def manifest_crate_names(manifest: Path) -> set[str]:
    """The ``name-version.crate`` files the Manifest carries a digest for."""
    return {
        line.split()[1]
        for line in manifest.read_text().splitlines()
        if line.startswith("DIST ") and line.split()[1].endswith(".crate")
    }


def rewrite_manifest(packages: list[tuple[str, str, str]]) -> None:
    """Replace every ``.crate`` DIST line, keeping the tarball's - see the module doc comment."""
    kept = [
        line
        for line in MANIFEST.read_text().splitlines()
        if not (line.startswith("DIST ") and line.split()[1].endswith(".crate"))
    ]
    lines = []
    for index, (name, version, sha256) in enumerate(packages, start=1):
        data = crate_bytes(name, version, sha256)
        lines.append(manifest_dist_line(name, version, data))
        if index % 50 == 0:
            print(f"  hashed {index}/{len(packages)}")
    MANIFEST.write_text("\n".join(sorted(lines + kept)) + "\n")
    print(f"updated {MANIFEST.relative_to(REPO_ROOT)} ({len(lines)} crates + the tag tarball)")


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
    lockfile = REPO_ROOT / "Cargo.lock"
    packages = packages_from_lockfile(lockfile)

    if "--manifest" in sys.argv[1:]:
        rewrite_manifest(packages)
        return 0

    crates = crates_from_lockfile(lockfile)
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

    # The Manifest, which drifts independently of the ebuild - see the module doc comment.
    expected = {f"{name}-{version}.crate" for name, version, _ in packages}
    present = manifest_crate_names(MANIFEST)
    if expected != present:
        for missing in sorted(expected - present):
            print(f"error: {MANIFEST.name} has no digest for {missing}", file=sys.stderr)
        for extra in sorted(present - expected):
            print(f"error: {MANIFEST.name} still carries {extra}", file=sys.stderr)
        print(
            "run: python3 scripts/generate_gentoo_crates.py --manifest",
            file=sys.stderr,
        )
        return 1

    if check_only:
        print(f"CRATES up to date ({len(crates)} crates)")
        print(f"Manifest up to date ({len(present)} crate digests)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
