# Packaging

Recipes for distributing `codediff` through package managers, kept in-repo so they version
alongside the code they build. Their status differs per target (table below): the Debian package
and the Homebrew tap are published by the release workflow; the Arch, Gentoo and Nix recipes are
not submitted to their distributions, whose submission targets (the AUR, a Gentoo overlay,
nixpkgs) live outside this repository; and the VS Code extension is a separate repository rather
than a recipe here.

| Target | Files | Status |
| --- | --- | --- |
| Arch (AUR) | `aur/PKGBUILD` | the AUR is closed to new submissions; the PKGBUILD builds locally with `makepkg -si` |
| Gentoo | `gentoo/dev-util/codediff/` | ready for an overlay |
| Debian/Ubuntu | `[package.metadata.deb]` in `../Cargo.toml` | **published** — signed apt repository at [ivankovic.github.io/codediff/apt](https://ivankovic.github.io/codediff/apt) |
| Nix / NixOS | `nix/package.nix`, `../flake.nix`, `../flake.lock` | works today via `nix run`; built by the Nix workflow |
| Homebrew | `homebrew/codediff.rb.in`, rendered by `../scripts/render_homebrew_formula.py` | tap at `ivankovic/homebrew-codediff`, pushed by the release workflow |
| VS Code | [codediff-vscode](https://github.com/ivankovic/codediff-vscode) | **published** — on the Marketplace and Open VSX |

## The one thing you cannot skip: checksums

The tarball hashes belong to the **v0.1.0** tag: they hash GitHub's tag tarball, so they can only
be regenerated after the next tag exists, and until then the recipes name the new version with the
old hash and do not build. `make check-versions` checks the version strings, not the hashes.

* `aur/PKGBUILD` carries the sha256 of the tag tarball
* `gentoo/dev-util/codediff/Manifest` carries one `DIST` line for the tag tarball and one for each
  vendored crate, each with its size, BLAKE2B and SHA512
* Nix needs a `hash =` only if you switch `package.nix` to `fetchFromGitHub`; as long as `src` is
  a parameter and `cargoLock.lockFile` points at the in-tree lock, there is nothing to hash

**The crate half is generated, and CI checks it** (`generate_gentoo_crates.py --check` covers both
the ebuild's `CRATES` and the Manifest). `--manifest` rebuilds the Manifest's crate lines from the
cargo cache, verifying each `.crate` against Cargo.lock's sha256 first:

```sh
python3 scripts/generate_gentoo_crates.py            # the ebuild's CRATES block
python3 scripts/generate_gentoo_crates.py --manifest  # the Manifest's crate digests
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

Do not hand-write a checksum: a wrong one looks correct until somebody's build fails.

## Decisions that apply to every recipe

**Source is the GitHub tag, not the crates.io tarball.** `Cargo.toml`'s `include` list ships only
the source, the licenses and the README - not `tests/**`, `src/bin/**`
or `src/test/data/**` - so a package built from crates.io has no test suite to run in its check
phase. The GitHub tag tarball has them — at the cost of also carrying `research/` and the fixture
corpus in the download.

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

`CRATES=` lists every dependency crate and is **generated, not edited**:

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
every one of the 300-odd dependency crates — 23 tree-sitter grammars among them — to be packaged as
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

**Both architectures are built natively**, on `ubuntu-22.04` and `ubuntu-22.04-arm`, unlike the
plain binaries in the same workflow, which reach aarch64 by cross-compiling. 22.04 rather than the
latest runner because `depends = "$auto"` writes the build machine's glibc version into the
package as its floor: 2.35 admits Ubuntu 22.04 and Debian 12, 2.39 would not. Two steps here cannot
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

1. A `v*` tag runs `release.yml`, which creates the release as a draft and has the `deb` matrix
   build and upload both architectures into it.
2. Its `checksums` job, after every upload, publishes the draft; its `apt` job — `needs:
   checksums`, so strictly after that — dispatches `pages.yml`.
3. `pages.yml` downloads every recent non-draft release's `.deb`, rebuilds the tree, signs it, and
   deploys it alongside the mapping site.

Step 2 exists because the obvious alternative does not work: a `release: published` trigger would
fire before the packages are attached, and `pages.yml` skips drafts. And the apt tree is deployed
by `pages.yml` rather than by `release.yml` because Pages has a single deployment for the whole
site — two workflows deploying separately would each erase the other.

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

**Check that the secret is not empty**, because nothing else will tell you:

```sh
gpg --armor --export-secret-keys '<KEYID>' | wc -c   # thousands of bytes, never 0
```

`gh secret set` accepts empty stdin and stores a secret that lists normally but expands to nothing;
pages.yml rejects a value that is not an armoured private key block.

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

`flake.lock` is committed, so `nix run github:ivankovic/codediff` builds against one pinned
nixpkgs rather than whatever `nixos-unstable` is that day. `nix flake update` moves the pin; do
it deliberately, in a commit of its own.

**CI builds the recipe.** `.github/workflows/nix.yml` runs `nix flake check` and `nix build` on
every change to the flake, the derivation or the Cargo files, and once a week to catch nixpkgs
moving under the lock file. It then checks the binary runs and the man page and completions are
installed. About twelve minutes uncached, which is why it is its own workflow and not a CI job.

The derivation's check phase runs the library tests under cargo-nextest, as CI and the `Makefile`
do, with `git` as a check-time input: the git review tests spawn `git` and change the working
directory, which is safe in a process of their own and not under plain `cargo test`.

**Building it locally without Nix installed.** The official image works through podman or
docker; the named volume keeps the store between runs so a retry only rebuilds codediff:

```
podman run --rm -it -v "$PWD":/src -w /src -v codediff-nix:/nix docker.io/nixos/nix:latest \
  sh -c 'git config --global --add safe.directory /src && \
         nix --extra-experimental-features "nix-command flakes" build .#codediff -L'
```

Flakes see only git-tracked files, so a new fixture or source file has to be `git add`ed before
the build sees it. The `result` link it leaves at the root is ignored.

## Homebrew

A tap, not homebrew-core: `brew install ivankovic/codediff/codediff` taps
[`ivankovic/homebrew-codediff`](https://github.com/ivankovic/homebrew-codediff) and installs from
it. The formula installs the release tarballs rather than building from source, so a user gets
the same attested binary every other route ships in seconds, instead of compiling every grammar
under fat LTO on their own machine. homebrew-core would not take a binary formula; a personal tap
routinely does. On macOS it installs the Apple Silicon or Intel build; on Linux the static musl
build, which runs on any distribution.

`homebrew/codediff.rb.in` is the source of truth, kept here so it versions with the code. It is a
template: the version and the four checksums come from the release's `SHA256SUMS.txt`, which
only exists once the release does. `scripts/render_homebrew_formula.py` fills them in and refuses
to leave a placeholder behind, and release.yml's `homebrew` job runs it after the checksums job
and pushes `Formula/codediff.rb` to the tap.

The push needs a fine-grained personal access token with **Contents: read and write** on the
tap repository alone, stored as the `HOMEBREW_TAP_TOKEN` secret of this repository. Without it the
job prints a warning and the release proceeds; render and push by hand then:

```sh
gh release download v<version> --pattern SHA256SUMS.txt
python3 scripts/render_homebrew_formula.py --version <version> --sums SHA256SUMS.txt \
  --out ../homebrew-codediff/Formula/codediff.rb
```

The man page and completions are generated at install time by the installed binary, as every
other recipe does; `brew test codediff` runs a real headless diff.

## Release checklist

1. Bump `version` in `Cargo.toml`, then `cargo update --workspace` so `Cargo.lock` follows.
2. `python3 scripts/generate_gentoo_crates.py` (the ebuild's `CRATES` block) and `--manifest` (the
   Manifest's crate digests, from the local cargo cache), and `git mv` the ebuild to the new
   version.
3. Update `pkgver` in `aur/PKGBUILD` and the fallback `version` in `nix/package.nix`.
   `make check-versions` passes once all three agree with `Cargo.toml`; `deploy-checks` and CI
   run it too.
4. Give `CHANGELOG.md`'s section for the version its release date: `## [x.y.z] - YYYY-MM-DD`.
   The release workflow takes the release notes from that section and fails on `unreleased`.
5. `nix flake update` if the nixpkgs pin should move with this release, and commit the lock
   file; the Nix workflow builds the result before the tag.
6. `make deploy` — publishes to crates.io, tags, and triggers the release workflow, which creates
   the release as a draft and publishes it once every asset is attached.
7. Now that the tag tarball exists, regenerate the two tarball hashes with `updpkgsums` and
   `ebuild ... manifest` (see "The one thing you cannot skip" above). **Not from
   `SHA256SUMS.txt`**: that file covers the release assets, and the source tarball is not one.
8. Push the updated Gentoo recipe to the overlay. The Homebrew tap updates itself from the
   release workflow (see "Homebrew" above); check that
   [`Formula/codediff.rb`](https://github.com/ivankovic/homebrew-codediff/blob/main/Formula/codediff.rb)
   names the new version.
9. Check that the apt repository picked the release up — `curl -s
   https://ivankovic.github.io/codediff/apt/dists/stable/main/binary-amd64/Packages | grep ^Version`
   should name the new version. It refreshes itself (step 2 above), so this is a check, not a task;
   if it is stale, re-run the Pages workflow.
