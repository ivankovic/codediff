# Packaging

Recipes for distributing `codediff` through system package managers. **None of the system-package
recipes is submitted anywhere yet** — these are the source of truth for them, kept in-repo so they
version alongside the code they build. The actual submission targets (the AUR, a Gentoo overlay,
nixpkgs) all live outside this repository. The one entry below that *is* published is the VS Code
extension, which is a separate repository rather than a recipe here.

| Target | Files | Status |
| --- | --- | --- |
| Arch (AUR) | `aur/PKGBUILD` | ready to submit |
| Gentoo | `gentoo/dev-util/codediff/` | ready for an overlay |
| Debian/Ubuntu | `[package.metadata.deb]` in `../Cargo.toml` | **published** — signed apt repository at [ivankovic.github.io/codediff/apt](https://ivankovic.github.io/codediff/apt) |
| Nix / NixOS | `nix/package.nix`, `../flake.nix` | works today via `nix run` |
| VS Code | [`vscode.md`](vscode.md) | **published** — v0.0.1 on the Marketplace and Open VSX, built from [codediff-vscode](https://github.com/ivankovic/codediff-vscode) |

## The one thing you cannot skip: checksums

Real for v0.0.14, and **every one of them has to be regenerated on the next version bump**:

* `aur/PKGBUILD` carries the sha256 of the v0.0.14 tag tarball
* `gentoo/dev-util/codediff/Manifest` carries 295 `DIST` lines - the tag tarball plus all 294
  vendored crates, each with its size, BLAKE2B and SHA512
* Nix needs a `hash =` only if you switch `package.nix` to `fetchFromGitHub`; as long as `src` is
  a parameter and `cargoLock.lockFile` points at the in-tree lock, there is nothing to hash

**The crate half is generated now, and CI checks it.** The Manifest's 294 crate digests drifted
silently through every dependency bump between v0.0.13 and v0.0.14 - 65 of them named older
versions and one crate had no line at all - because `generate_gentoo_crates.py --check` validated
the ebuild's `CRATES` list and nothing looked at the Manifest. It checks both now, and
`--manifest` rebuilds the crate lines from the cargo cache, verifying each `.crate` against the
sha256 Cargo.lock already records before hashing it:

```sh
python3 scripts/generate_gentoo_crates.py            # the ebuild's CRATES block
python3 scripts/generate_gentoo_crates.py --manifest  # the Manifest's 294 crate digests
```

The two *tarball* hashes are the part no script can do ahead of time, because they hash the GitHub
tag tarball and that does not exist until the release is tagged. Regenerate them with the real
tools where you have them:

```sh
cd packaging/aur && updpkgsums                      # rewrites sha256sums=() in place
ebuild gentoo/dev-util/codediff/codediff-<version>.ebuild manifest
nix-prefetch-url --unpack https://github.com/ivankovic/codediff/archive/refs/tags/v<version>.tar.gz
```

**Not from `SHA256SUMS.txt`.** That asset hashes the *release assets* - the prebuilt binaries, the
`.deb`, the completions tarball - and GitHub's auto-generated source tarball is not one of them
(see the `checksums` job in `.github/workflows/release.yml`, which hashes exactly what
`gh release download` returns). The recipes here all build from the source tarball, so its hash
has to come from the tarball itself.

Do not hand-write a checksum. A wrong one looks correct until the moment somebody's build fails.

Computing one from the downloaded artifact is not hand-writing it, and is what those tools do
anyway - but check your work. The v0.0.14 values were produced without `updpkgsums`/`ebuild`
available, so: the tag tarball was fetched twice and both fetches hashed identically
(`b9192d9c…`, 68,557,321 bytes), its BLAKE2B/SHA512 were cross-checked against `b2sum` and
`sha512sum`, every crate file was verified against the sha256 `Cargo.lock` already records for it
before being hashed, and `--check` confirmed both the ebuild's `CRATES` list and the Manifest's
crate set against `Cargo.lock` afterwards (294 = 294, no drift).

## Decisions that apply to every recipe

**Source is the GitHub tag, not the crates.io tarball.** `Cargo.toml`'s `exclude` list drops
`tests/**`, `src/bin/**` and `src/test/data/**` from the published crate, so a package built from
crates.io has no test suite to run in its check phase. The GitHub tag tarball has them — at the
cost of also carrying `research/` and the fixture corpus in the download.

**Tests are restricted to `--lib`.** The fixture-corpus tests are the accuracy benchmark: they
need the `test-fixtures` feature and substantial time and memory. That is not what a packaging
sanity check is for.

**Default features only.** `stats` and `test-fixtures` gate dataset-analysis dev tools that pull in
git2 (OpenSSL, libssh2) and a bundled SQLite. They are not part of the shipped product, so no
recipe exposes them as a build option.

**Completions and the man page are generated, never hand-written.** Each recipe runs
`codediff util man` and `codediff util completions <shell>` against the binary it just built, so
they track the real flag list. This assumes a **native** build — under cross-compilation the target
binary cannot be executed, and these would have to come from a host build instead (which is exactly
what the release workflow's `assets` job does for the prebuilt tarballs).

**Builds are slow, and that is expected.** Every tree-sitter grammar compiles from C, and the
release profile sets `lto = "fat"` with `codegen-units = 1`. Minutes, not seconds.

## Gentoo

`CRATES=` lists all 293 dependency crates and is **generated, not edited**:

```sh
python3 scripts/generate_gentoo_crates.py            # rewrite the block
python3 scripts/generate_gentoo_crates.py --check    # fail if stale (for CI)
```

Run this after any `Cargo.lock` change. A stale list produces a package that fails to build for
users while looking fine in review. `pycargoebuild` does the same job if you have it installed.

The `LICENSE` variable enumerates the vendored crates' licenses alongside the package's own
`AGPL-3+`; re-check it if the dependency set changes substantially.

## Debian

The `.deb` is built with [`cargo-deb`](https://github.com/kornelski/cargo-deb), attached to each
GitHub release for amd64 and arm64, and served from an apt repository on GitHub Pages. It is
**unofficial**, and the distinction matters: a package in the Debian archive proper would require
every one of the 293 dependency crates — 24 tree-sitter grammars among them — to be packaged as
`librust-*-dev` first. Almost none are. That path is not reachable, so this is a `cargo-deb`
artifact served from our own repository, not a route into Debian.

To build one locally:

```sh
cargo install cargo-deb
cargo build --release --bin codediff
mkdir -p target/dist
./target/release/codediff util man                > target/dist/codediff.1
./target/release/codediff util completions bash   > target/dist/codediff.bash
./target/release/codediff util completions zsh    > target/dist/_codediff
./target/release/codediff util completions fish   > target/dist/codediff.fish
cargo deb --no-build
```

The generation step is not optional: `cargo-deb` copies assets from disk and cannot run the binary
itself, so those four files must exist before it runs.

**Both architectures are built natively**, on `ubuntu-latest` and `ubuntu-24.04-arm`, unlike the
plain binaries in the same workflow, which reach aarch64 by cross-compiling. Two steps here cannot
cross: `depends = "$auto"` resolves the built ELF's needs against the packages installed on the
build machine, and the man page and completions come from *running* the binary. Cross-building the
arm64 `.deb` on an x86-64 host would stamp the host's libc version onto an arm64 package.

### The apt repository

`scripts/build_apt_repo.sh` turns a directory of `.deb` files into a signed, static apt tree —
pool, per-architecture `Packages`, a `Release` signed both inline (`InRelease`) and detached
(`Release.gpg`), and the public key dearmoured for `signed-by`. The Pages workflow calls it; it
takes no repository state and can be run against any directory of packages:

```sh
export APT_GPG_PRIVATE_KEY="$(gpg --armor --export-secret-keys <KEYID>)"
scripts/build_apt_repo.sh --debs path/to/debs --out /tmp/apt
```

**Nothing about the repository is committed.** The pool is rebuilt on every Pages run from the
`.deb` assets of the last five releases, so the published tree is a pure function of the releases
that exist. Losing it costs one workflow run. Five is a `KEEP_RELEASES` in `pages.yml`, set
against the 1 GB soft limit on a Pages site rather than for any packaging reason.

The two halves are wired together in an order that matters:

1. A `v*` tag runs `release.yml`. The `deb` matrix builds and uploads both architectures.
2. Its `apt` job — `needs: deb`, so strictly after those uploads — dispatches `pages.yml`.
3. `pages.yml` downloads every recent release's `.deb`, rebuilds the tree, signs it, and deploys
   it alongside the mapping site.

Step 2 exists because the obvious alternative does not work: a `release: published` trigger fires
when `softprops/action-gh-release` creates the release from whichever matrix job finishes first,
which is long before the packages are attached. And the apt tree is deployed by `pages.yml` rather
than by `release.yml` because Pages has a single deployment for the whole site — two workflows
deploying separately would each erase the other.

### The signing key

One-time setup, and the only manual step in any of this. The key signs nothing but this
repository's `Release` file, so it wants no expiry — an expired key breaks `apt update` for every
user on a date nobody is watching, which is what the trailing `never` is for:

```sh
gpg --batch --pinentry-mode loopback --passphrase '' \
    --quick-gen-key 'CodeDiff apt repository <marko@ivankovic.me>' rsa4096 sign never
gpg --armor --export-secret-keys '<KEYID>' | gh secret set APT_GPG_PRIVATE_KEY
```

`--pinentry-mode loopback --passphrase ''` is not optional shorthand: `--batch` on its own makes
gpg reach for a pinentry it has no terminal for and fail with `Inappropriate ioctl for device`.
Drop all three flags to be prompted for a passphrase instead, and store it as a second secret,
`APT_GPG_PASSPHRASE` — the script signs without one when it is unset. Note what that passphrase
buys, though: it would sit in the same secret store as the key it protects.

A repository secret is enough. `pages.yml` reads it in its `build` job, which has no environment,
so an environment secret would need that job attached to one first.

The private key exists only in that secret. **Back it up somewhere you control**: losing it means
generating a new one, and every user who added the old key gets a signature failure on their next
`apt update` until they re-fetch `codediff-archive-keyring.gpg`. That is also what makes rotation
expensive, so rotate on evidence, not on a schedule.

## Nix

`nix run github:ivankovic/codediff` works against the repository directly — no tag, no release
artifact, no vendor hash, because `cargoLock.lockFile` vendors straight from the committed
`Cargo.lock`. `nix develop` gives a shell with the toolchain the `Makefile` targets expect.

A nixpkgs submission would take `nix/package.nix` as-is but swap `src` for a `fetchFromGitHub` call
and `cargoLock.lockFile` for a `cargoHash`, since nixpkgs does not carry the lock file. The
`maintainers` list is deliberately empty until somebody agrees to be on it.

## Release checklist

1. Bump `version` in `Cargo.toml`.
2. `python3 scripts/generate_gentoo_crates.py` and rename the ebuild to match the new version.
3. Update `pkgver` in `aur/PKGBUILD` and the fallback `version` in `nix/package.nix`.
4. `make deploy` — publishes to crates.io, tags, and triggers the release workflow.
5. Read the new `SHA256SUMS.txt` off the release and fill in `sha256sums` / the Gentoo `Manifest`.
6. Push the updated recipes to the AUR and the overlay.
7. Check that the apt repository picked the release up — `curl -s
   https://ivankovic.github.io/codediff/apt/dists/stable/main/binary-amd64/Packages | grep ^Version`
   should name the new version. It refreshes itself (step 2 above), so this is a check, not a task;
   if it is stale, re-run the Pages workflow.
