# CodeDiff

Fast, robust, syntax aware code diffing.

# Guiding principles

## Robust

CodeDiff must be able to process 100% of all commits in the full test dataset.

The full test dataset contains the git commit history, as available on the main branch, for about
7400 open source git repositories. The list of repositories was extracted from the Gentoo Linux
distribution and is available in list_of_repositories.csv.

A smaller list of 100 repositories, called "small" dataset is available for faster iterations when
debugging.

## Fast

CodeDiff must produce a diff in under 400ms for 99.99% of all commits in the full test dataset.

In code, I accept less readable, more complex code if it is faster.

Benchmarks are used to make sure performance doesn't regress.

# Code quality

No Rust lint errors are allowed.

# Testing

CodeDiff has two types of tests:

- Automated tests that should be run frequently during coding.

- Semi-automated tests that run on the small and full dataset that take some time to run and should
  be run when appropriate, definitely before any release.

## Automated tests

Tests in src/ must run in uner 5 seconds. Most tests should run under 1 second.

Tests in tests/ must run in under 10 seconds. Most tests should run under 5 seconds.

### How should tests handle dependencies?

*No mocks*. Mocks prevent testing through the interface and are brittle.

Ideally, the real implementation is used.

When necessary, e.g. for filesystem access, fake in-memory dependencies are used.

# License

Copyright (C) 2026 Marko Ivankovic

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU Affero General Public License as published
by the Free Software Foundation.

See the LICENSE file for the full text of the License.

## Can't use AGPL software?

Contact me for options.
