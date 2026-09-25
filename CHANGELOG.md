# Changelog

All notable changes to CodeDiff. The format follows [Keep a Changelog](https://keepachangelog.com/),
and the version numbers follow [Semantic Versioning](https://semver.org/) as far as a 0.x
release does: a minor bump may change the JSON output or the library API, a patch bump does not.

The release workflow takes a release's notes from its section here, and refuses to cut a release
whose heading still says `unreleased`.

## [0.1.0] - 2026-09-25

The first release ready for users!

Ready for:

-  Daily usage as a `git difftool` tool!
-  Daily usage in an IDE!
-  Integration into batch pipelines and LLM agents!

Things that you can do, but expect changes:

-  Integration as a library into other products. The API is not fixed and might change.

### Metrics:

-  Speed: p50 7.6ms, p90 78.7ms, p99 347ms, slowest 1,355ms. 100ms is the 92.7th percentile
-  Robust: 99.95%
-  Accurate: 70.6% perfect, 88.6% <= 1% off

## [0.0.14] and earlier

These were pre-release releases, only really used by 1 person.

See the [GitHub releases](https://github.com/ivankovic/codediff/releases).
