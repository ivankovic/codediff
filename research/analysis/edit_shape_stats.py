#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
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
"""
Measures the *shape of real-world file edits* over the cloned corpus: how many lines a commit
touches, how many files it touches, and how much of a file an edit changes. This answers the
introductory paper's Section 3 ("Shape of real-world file edits"), which previously cited only
Arafat and Riehle's commit-size distribution and carried a TODO for our own numbers.

Population: the most recent `--max-commits` non-merge commits of each clone under
`repositories/`, 50 by default to match the corpus's stated clone depth. The cap is not a
performance measure, it is what makes the population the paper describes. These clones are shallow
but not uniformly so - `torvalds-linux.git` alone carries 1.29M of the corpus's 2.31M reachable
commits - so an uncapped walk reports the Linux kernel's edit habits with the other 99
repositories as rounding error. Capping equalises repositories and matches the paper's CorpusCloneDepth.

Within that window this is a census, not a sample, and deliberately not the stratified draw in
`data/samples/sampled_code_pairs_*.csv`, which over-represents large files by construction and so
cannot answer "how big is a typical edit".

Churn - what share of a file an edit rewrites - is derived, not joined. `git log --numstat` gives
added and removed; counting the newlines of the after-side blob (one `git cat-file --batch` per
repository) gives `lines_after`, and `lines_before = lines_after - added + removed` follows
exactly, so every edit in the window gets a fraction. An earlier version instead joined against
`stats.sqlite`'s `commits` table, which covered 6,000 of 19.7M edits - 0.03%, and selected
differently - and was not a usable denominator. That table cannot supply churn directly either:
its `lines_added`, `lines_removed`, `lines_changed` and three `nodes_*` columns are all zero for
every row, hardcoded by `commit_stats.rs` ("the actual diff processing will be implemented later").

This measurement does not parse anything, so it reports no AST-node churn. The line-level fraction
is a proxy for it, and the paper labels it as one.

Usage (from research/):  uv run ./analysis/edit_shape_stats.py [--mode small] [--repositories DIR]
"""

import argparse
import array
import collections
import csv
import os
import subprocess
import sys

# Extension -> language, covering the languages the fixture corpus and the empirical study use.
# Written out here rather than shelling into the Rust `detect_language_from_path` because this
# script's only use for it is the code/not-code split: an edit to a Markdown changelog and an edit
# to a function body are different populations, and the paper's own Figure 1 already makes that
# split for files at rest. Anything not listed is counted as not-code.
CODE_EXTENSIONS = {
    "c": "C", "h": "C",
    "cc": "CPP", "cpp": "CPP", "cxx": "CPP", "hpp": "CPP", "hh": "CPP",
    "cs": "CSharp",
    "css": "CSS", "scss": "CSS",
    "go": "Go",
    "html": "HTML", "htm": "HTML",
    "java": "Java",
    "js": "JavaScript", "mjs": "JavaScript", "cjs": "JavaScript",
    "jsx": "JavaScript",
    "kt": "Kotlin", "kts": "Kotlin",
    "lua": "Lua",
    "php": "PHP",
    "py": "Python",
    "r": "R",
    "rb": "Ruby",
    "rs": "Rust",
    "scala": "Scala", "sc": "Scala",
    "sh": "ShellScript", "bash": "ShellScript", "zsh": "ShellScript",
    "swift": "Swift",
    "ts": "TypeScript",
    "tsx": "TSX",
    "vim": "Vimscript",
}


def language_of(path):
    """The language of `path`, or None if it is not a code file by CODE_EXTENSIONS."""
    _, _, ext = path.rpartition(".")
    return CODE_EXTENSIONS.get(ext.lower())


def numstat_rows(repo, max_commits):
    """Every (commit, path, added, removed) in `repo`'s cloned history, non-merge commits only.

    Streams `git log`'s stdout line by line rather than capturing it. The clones here are depth-
    limited per branch, not per repository, so a single repository's log can run to millions of
    lines and hundreds of megabytes; `capture_output=True` holds all of that in memory at once,
    which is most of what made the first version of this script peak at 7.7 GB.

    `--numstat` reports `-\t-\t<path>` for a binary file; those carry no line counts and are
    skipped. A rename is reported with a brace-expanded path (`a/{b => c}/d`); the post-rename
    path is what the row is attributed to, since that is the file the edit produced.
    """
    proc = subprocess.Popen(
        ["git", "-C", repo, "log", "--no-merges", "--numstat", "--format=C%H"]
        + ([f"-n{max_commits}"] if max_commits else []),
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
        text=True,
        errors="replace",
        bufsize=1 << 20,
    )
    commit = None
    try:
        for line in proc.stdout:
            line = line.rstrip("\n")
            if line.startswith("C"):
                commit = line[1:]
                continue
            if not line.strip() or commit is None:
                continue
            parts = line.split("\t", 2)
            if len(parts) != 3:
                continue
            added, removed, path = parts
            if added == "-" or removed == "-":
                continue
            if " => " in path:
                # A rename, as either "before => after" or "dir/{before => after}/file". Rewriting
                # each brace group to its right-hand side yields the post-rename path, which is the
                # file the edit produced and therefore the one to attribute the lines to.
                while "{" in path and "}" in path:
                    head, _, rest = path.partition("{")
                    group, _, tail = rest.partition("}")
                    path = head + group.split(" => ")[-1] + tail
                path = path.split(" => ")[-1]
            yield commit, path.strip(), int(added), int(removed)
    finally:
        proc.stdout.close()
        if proc.wait() != 0:
            print(f"note: git log failed in {os.path.basename(repo)}", file=sys.stderr)


def after_line_counts(repo, blobs):
    """`(commit, path)` -> the after-side file's line count, for every pair in `blobs`.

    One `git cat-file --batch` per repository rather than one `git show` per file: the batch
    protocol writes "<oid> <type> <size>\\n<contents>\\n" per request on one long-lived process,
    which is the difference between a few seconds and a few minutes over a corpus this size.
    A request git cannot resolve answers "<rev> missing" and is skipped.
    """
    if not blobs:
        return {}
    requests = "".join(f"{commit}:{path}\n" for commit, path in blobs)
    proc = subprocess.Popen(
        ["git", "-C", repo, "cat-file", "--batch"],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
    )
    out, _ = proc.communicate(requests.encode())

    counts = {}
    offset = 0
    for commit, path in blobs:
        newline = out.find(b"\n", offset)
        if newline == -1:
            break
        header = out[offset:newline]
        offset = newline + 1
        if header.endswith(b"missing"):
            continue
        try:
            size = int(header.split()[-1])
        except (IndexError, ValueError):
            break
        counts[(commit, path)] = out.count(b"\n", offset, offset + size)
        # git writes the object, then a newline of its own.
        offset += size + 1
    return counts


def percentile(values, q):
    """The `q`th percentile by nearest rank, on an already-sorted list."""
    if not values:
        return None
    index = min(len(values) - 1, max(0, round(q / 100 * (len(values) - 1))))
    return values[index]


def latex_number(value):
    """1234567 -> "1{,}234{,}567", this project's papers' LaTeX-safe thousands separator.
    Mirrors `paper_variables.py::latex_number`."""
    return f"{value:,}".replace(",", "{,}")


class Accumulator:
    """Everything the paper quotes, accumulated one file edit at a time.

    Deliberately keeps no per-row records. The population here is millions of file edits, so the
    first version of this script - which built a list of dicts and wrote every row to CSV - peaked
    at 7.7 GB and would have produced a several-hundred-megabyte artifact nobody can commit. What
    the statistics actually need is one integer per code-file edit, one running total per commit,
    and one fraction per edit whose file size is known; the integers live in `array`s of machine
    ints rather than Python lists of boxed ones.
    """

    def __init__(self):
        self.changed_by_language = collections.defaultdict(lambda: array.array("l"))
        self.per_commit = collections.Counter()
        self.files_per_commit = collections.Counter()
        self.churn = array.array("d")
        self.kinds = collections.Counter()
        self.total_edits = 0
        self.code_edits = 0
        self.modified_edits = 0

    def note_edit(self):
        """One edited file of any kind, code or not - the denominator of the code share."""
        self.total_edits += 1

    def _classify(self, added, removed, lines_after):
        """`modified`, `created` or `deleted`. A creation has nothing before it and a deletion
        nothing after, so neither is a diff in the sense this paper measures: there is no pair of
        versions to map between. They are counted and then excluded from every distribution."""
        lines_before = lines_after - added + removed
        if lines_before <= 0:
            return "created"
        if lines_after <= 0:
            return "deleted"
        return "modified"

    def add(self, commit, path, added, removed, lines_after):
        """One file edit. `lines_after` is the after-side line count, or None if git could not
        resolve the blob; `lines_before` follows from it exactly, since numstat's own added and
        removed are what produced it."""
        language = language_of(path)
        if language is None:
            return
        self.code_edits += 1
        # A blob git cannot resolve is the file's deletion: it does not exist at that commit.
        lines_after = 0 if lines_after is None else lines_after
        kind = self._classify(added, removed, lines_after)
        self.kinds[kind] += 1
        if kind != "modified":
            return

        changed = added + removed
        self.modified_edits += 1
        self.changed_by_language[language].append(changed)
        self.per_commit[commit] += changed
        self.files_per_commit[commit] += 1
        denominator = max(lines_after - added + removed, lines_after)
        if denominator > 0:
            self.churn.append(min(1.0, changed / denominator))

    def _all_changed(self):
        merged = array.array("l")
        for values in self.changed_by_language.values():
            merged.extend(values)
        return sorted(merged)

    def summary(self, repositories):
        per_file = self._all_changed()
        commit_totals = sorted(self.per_commit.values())
        commit_files = sorted(self.files_per_commit.values())
        churn = sorted(self.churn)

        def share(values, limit):
            if not values:
                return None
            return f"{sum(1 for v in values if v <= limit) / len(values) * 100:.1f}"

        out = {
            "EditsRepositories": repositories,
            "EditsCommits": latex_number(len(commit_totals)),
            "EditsFileEdits": latex_number(len(per_file)),
            "EditsCodeFileEdits": latex_number(self.code_edits),
            "EditsModifiedSharePct": (
                f"{self.modified_edits / self.code_edits * 100:.1f}" if self.code_edits else None
            ),
            "EditsLanguages": len(self.changed_by_language),
            "EditsCodeSharePct": (
                f"{self.code_edits / self.total_edits * 100:.1f}" if self.total_edits else None
            ),
            "EditsLinesPerFilePFifty": percentile(per_file, 50),
            "EditsLinesPerFilePNinety": percentile(per_file, 90),
            "EditsLinesPerFilePNinetyNine": latex_number(percentile(per_file, 99)),
            "EditsLinesPerFileMax": latex_number(per_file[-1]) if per_file else None,
            "EditsFileEditsUnderTenPct": share(per_file, 10),
            "EditsLinesPerCommitPFifty": percentile(commit_totals, 50),
            "EditsLinesPerCommitPNinety": latex_number(percentile(commit_totals, 90)),
            "EditsLinesPerCommitPNinetyNine": latex_number(percentile(commit_totals, 99)),
            "EditsLinesPerCommitMax": latex_number(commit_totals[-1]) if commit_totals else None,
            "EditsCommitsUnderTenPct": share(commit_totals, 10),
            "EditsFilesPerCommitPFifty": percentile(commit_files, 50),
            "EditsFilesPerCommitPNinety": percentile(commit_files, 90),
            "EditsFilesPerCommitPNinetyNine": percentile(commit_files, 99),
        }
        if churn:
            out |= {
                "EditsChurnScored": latex_number(len(churn)),
                "EditsChurnPFiftyPct": f"{percentile(churn, 50) * 100:.1f}",
                "EditsChurnPNinetyPct": f"{percentile(churn, 90) * 100:.1f}",
                "EditsChurnUnderFivePct": share([c * 100 for c in churn], 5),
                "EditsChurnUnderTwentyPct": share([c * 100 for c in churn], 20),
            }
        return out

    def language_rows(self):
        """One row per language - the committable artifact, in place of the per-edit rows."""
        for language, values in sorted(self.changed_by_language.items()):
            ordered = sorted(values)
            yield {
                "language": language,
                "file_edits": len(ordered),
                "lines_changed": sum(ordered),
                "lines_p50": percentile(ordered, 50),
                "lines_p90": percentile(ordered, 90),
                "lines_p99": percentile(ordered, 99),
                "lines_max": ordered[-1],
            }


def write_paper_fragment(summary, path):
    """The `\\newcommand` block paper_variables.py merges in. Same contract as every other
    fragment writer: values only, no percent signs - the paper adds those."""
    with open(path, "w") as f:
        f.write("% Auto-generated by research/analysis/edit_shape_stats.py. Do not edit by hand.\n")
        for name, value in summary.items():
            if value is not None:
                f.write(f"\\newcommand{{\\{name}}}{{{value}}}\n")
    print(f"Paper fragment written to {path}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    # Defaults match the research Makefile's own REPOSITORIES_DIR/RESEARCH_DIR for MODE=small;
    # the `edit-shape` target passes both explicitly so MODE keeps working.
    parser.add_argument("--repositories", default="/var/tmp/research/small/repositories")
    # Matches the corpus's stated clone depth. 0 walks the whole clone,
    # which lets one very deep repository dominate - see this module's doc comment.
    parser.add_argument("--max-commits", type=int, default=50)
    parser.add_argument("--output", default=None)
    parser.add_argument("--fragment", default=None)
    args = parser.parse_args()

    here = os.path.dirname(os.path.abspath(__file__))
    research_dir = os.path.dirname(here)
    repositories = args.repositories
    output = args.output or os.path.join(research_dir, "data", "corpus_stats", "edit_shape.csv")
    fragment = args.fragment or os.path.join(research_dir, "plots", "variables_edits.tex")

    if not os.path.isdir(repositories):
        sys.exit(f"no repositories under {repositories} - run `make fetch MODE=<mode>` first")

    accumulator = Accumulator()
    names = sorted(os.listdir(repositories))
    counted = 0
    for i, name in enumerate(names, 1):
        repo = os.path.join(repositories, name)
        if not os.path.isdir(repo):
            continue
        counted += 1
        edits = []
        for row in numstat_rows(repo, args.max_commits):
            # Counted before the code filter, so EditsCodeSharePct keeps a real denominator.
            accumulator.note_edit()
            if language_of(row[1]):
                edits.append(row)
        # Resolved per repository rather than per edit: one batch process, one pass.
        lines = after_line_counts(repo, [(commit, path) for commit, path, _, _ in edits])
        for commit, path, added, removed in edits:
            accumulator.add(commit, path, added, removed, lines.get((commit, path)))
        if i % 10 == 0:
            print(
                f"  {i}/{len(names)} repositories, {accumulator.code_edits} code-file edits",
                file=sys.stderr,
            )

    if not accumulator.code_edits:
        sys.exit("no code-file edits found")

    rows = list(accumulator.language_rows())
    os.makedirs(os.path.dirname(output), exist_ok=True)
    with open(output, "w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=list(rows[0]))
        writer.writeheader()
        writer.writerows(rows)
    print(f"per-language edit shape for {len(rows)} languages written to {output}")

    summary = accumulator.summary(counted)
    for name, value in summary.items():
        print(f"  {name:32} {value}")
    write_paper_fragment(summary, fragment)


if __name__ == "__main__":
    main()
