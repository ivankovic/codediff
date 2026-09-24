# Changelog

All notable changes to CodeDiff. The format follows [Keep a Changelog](https://keepachangelog.com/),
and the version numbers follow [Semantic Versioning](https://semver.org/) as far as a 0.x
release does: a minor bump may change the JSON output or the library API, a patch bump does not.

The release workflow takes a release's notes from its section here, and refuses to cut a release
whose heading still says `unreleased`.

## [0.1.0] - unreleased

The first release announced beyond the people who were already using it. The engine, its quality
gates and the release pipeline were in place at 0.0.14; this release fixes what a first-time user
on a fresh machine would have hit.

### Fixed

- Headless output no longer panics when the reader closes the pipe early (`codediff a b | head`,
  or quitting `less` before the end of a long diff). It exits quietly instead.
- `q` only quits from the viewer. Typing `q` in the search box, the go-to-line prompt or the file
  dialog's filter no longer ends the session.
- Windows: key-release events are ignored, so every keystroke no longer fires twice (`n` skipped
  two changes, `?` opened and closed the help).
- Every error exits 2, as `--help` documents. A wrong argument count, `--headless` or `--mode
  json` without files, and a failed setup wizard exited 1, which a `--exit-code` script reads as
  "the files differ".
- `codediff git configure` quotes the binary's path in `difftool.codediff.cmd`, so an install
  under a directory with a space in its name (`Program Files`, `My Tools`) works. Ctrl-D at any
  of the wizard's prompts aborts without writing anything; it used to be read as "accept the
  default".
- `NO_COLOR=` (set but empty) no longer disables colour, per [no-color.org](https://no-color.org).

### Changed

- The TUI writes no log file unless `RUST_LOG` is set. When it is, the log goes to
  `$XDG_STATE_HOME/codediff/log.txt` (by default `~/.local/state/codediff/log.txt`) instead of
  the world-shared `/tmp/codediff/log.txt`, which another user could have created first.
- `--mode` accepts exactly `tui`, `headless` or `json` (case-insensitive), and shell completions
  offer the three values. An unknown mode used to be accepted silently.
- `--tui-tick-rate` and `--tui-frame-rate` are hidden from `--help` and reject zero, negative and
  non-finite values, which used to panic.
- The deprecated no-op `--exact` flag is removed.
- Prebuilt Linux binaries and the amd64 `.deb` are built on Ubuntu 22.04 (glibc 2.35), so they
  run on Ubuntu 22.04, Debian 12 and RHEL 9. The 0.0.14 binaries needed glibc 2.39.
- Release archives contain `LICENSE` and `README.md` beside the binary, every release asset
  carries a GitHub build-provenance attestation (`gh attestation verify <file> --repo
  ivankovic/codediff`), and `cargo binstall codediff` installs the prebuilt binary.
- The published crate carries only what it builds from: the source, the browser viewer's page,
  the license and the README. Working notes, the research data and the fixture stubs' data are
  no longer in the crates.io tarball.
- `cargo doc` on docs.rs covers the `web` feature as well as the default `tui`.

### Added

- A panic hook for the TUI: on a panic it restores the terminal, prints the panic message where
  it can be read, and asks for a bug report at the issue tracker. A panic used to leave the
  message inside the alternate screen, where it was wiped as the program exited.
- This changelog. CI checks that the AUR, Gentoo and Nix recipes name the same version as
  `Cargo.toml`, tests the suite on macOS and Windows, checks the crate builds on the declared
  minimum Rust version, and packages the crate (`cargo package`) on every push.

### Engine and ground truth

- 142 more hand-authored ground-truth mappings for the Defects4J (Java) fixtures - 273 of its
  996 are now solved, 1,277 fixtures corpus-wide - each clamped to its measured accuracy, and
  human solutions that can say "no unique answer" or map N nodes to M.
- Painting rules that follow the corpus: operators, booleans and access modifiers are painted
  whole, and a renamed identifier is painted whole under `--full`.
- Every construction-time render option rebuilds the diff, so toggling one in the `M` panel
  shows what the option actually changes.
- Line coverage is measured per test as well as per area (`make coverage`).
- A signed apt repository on GitHub Pages, and the browser showcase of twenty real changes.

## [0.0.14] and earlier

See the [GitHub releases](https://github.com/ivankovic/codediff/releases).
