# Packaging

Recipes for distributing `codediff` through system package managers. **Nothing here is submitted
anywhere yet** — these are the source of truth for the recipes, kept in-repo so they version
alongside the code they build. The actual submission targets (the AUR, a Gentoo overlay, nixpkgs)
all live outside this repository.

| Target | Files | Status |
| --- | --- | --- |
| Arch (AUR) | `aur/PKGBUILD` | ready to submit; needs `sha256sums` |
| Gentoo | `gentoo/dev-util/codediff/` | ready for an overlay; needs a `Manifest` |
| Debian/Ubuntu | `[package.metadata.deb]` in `../Cargo.toml` | built by CI, attached to each release |
| Nix / NixOS | `nix/package.nix`, `../flake.nix` | works today via `nix run` |
| VS Code | [`vscode.md`](vscode.md) | requirements written up; extension not built |

## The one thing you cannot skip: checksums

Three of the four recipes need a hash of the release tarball, and **none of the placeholders in
this directory are real**:

* `aur/PKGBUILD` ships `sha256sums=('SKIP')`
* the Gentoo package has no `Manifest` at all
* Nix needs a `hash =` only if you switch `package.nix` to `fetchFromGitHub`

Every release publishes a `SHA256SUMS.txt` asset covering all of its artifacts (see the `checksums`
job in `.github/workflows/release.yml`). Read the values from there, or regenerate locally:

```sh
cd packaging/aur && updpkgsums                      # rewrites sha256sums=() in place
ebuild gentoo/dev-util/codediff/codediff-0.0.13.ebuild manifest
nix-prefetch-url --unpack https://github.com/ivankovic/codediff/archive/refs/tags/v0.0.13.tar.gz
```

Do not hand-write a checksum. A wrong one looks correct until the moment somebody's build fails.

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

The `.deb` is built with [`cargo-deb`](https://github.com/kornelski/cargo-deb) and attached to each
GitHub release. It is **unofficial**, and the distinction matters: a package in the Debian archive
proper would require every one of the 293 dependency crates — 24 tree-sitter grammars among them —
to be packaged as `librust-*-dev` first. Almost none are. That path is not reachable, so this is a
`cargo-deb` artifact, not a route into Debian.

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
