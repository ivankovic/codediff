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
"""Runs every check `.github/workflows/ci.yml` runs, locally, in this working tree.

Unlike `.githooks/pre-push` - which is a deliberately fast, hand-picked subset - this is a full
mirror, and it is a mirror *by construction*: the commands come from parsing ci.yml itself, not
from a copy of them kept in step. Adding a job or changing a flag in ci.yml changes what this runs,
with no second edit. That is the entire point; a hand-maintained duplicate of CI is worth less than
no local CI at all, because it goes stale silently and you find out from GitHub anyway.

What it cannot mirror is the runner: this uses the toolchain, OS and installed tools you already
have, where GitHub gets a clean ubuntu-latest with a pinned toolchain each time. So this catches
your mistakes, not "works on my machine" ones. `act` would close that last gap by running the
workflow in a container, at the cost of a from-scratch build of a crate whose release profile is
`lto = "fat"`, `codegen-units = 1` - tens of minutes per feature config, every time, with no
incremental `target/` to reuse. That trade is not worth it for the common case, which is "did I
break CI before I push".

Actions (`uses:`) have no local equivalent and are translated by the table below. An action this
script has never seen is a hard error rather than a silent skip: a new `uses:` in ci.yml is a
deliberate decision about what runs locally, and quietly ignoring it would reintroduce exactly the
drift this design exists to prevent.
"""

from __future__ import annotations

import argparse
import os
import shutil
import subprocess
import sys
import time
from pathlib import Path
from typing import Any

try:
    import yaml
except ImportError:  # pragma: no cover - environment problem, not a code path
    sys.exit(
        "scripts/ci_local.py needs PyYAML to read .github/workflows/ci.yml.\n"
        "Install it with `pip install pyyaml` (or your distro's python3-yaml package)."
    )

REPO_ROOT = Path(__file__).resolve().parent.parent
WORKFLOW = REPO_ROOT / ".github" / "workflows" / "ci.yml"


# How to prove a `dtolnay/rust-toolchain` component is present locally, keyed by the component name
# CI asks for.
COMPONENT_CHECKS = {
    "rustfmt": ["cargo", "fmt", "--version"],
    "clippy": ["cargo", "clippy", "--version"],
}


class StepSkipped(Exception):
    """A `uses:` step with nothing to do locally - carries the reason, for the log."""


def action_name(uses: str) -> str:
    """`owner/repo` from a `uses:` value, dropping the `@ref` and any subdirectory path."""
    return uses.split("@", 1)[0]


def check_tool(argv: list[str], what: str) -> None:
    """Fails the run if a tool CI installs for itself is missing here."""
    if shutil.which(argv[0]) is None:
        raise RuntimeError(f"{what} needs `{argv[0]}` on PATH, which CI installs for itself")
    result = subprocess.run(argv, capture_output=True, text=True, check=False)
    if result.returncode != 0:
        raise RuntimeError(f"{what}: `{' '.join(argv)}` failed - is it installed?")


def tool_version(argv: list[str]) -> str:
    """First line of a `--version` style output, for the local-vs-CI comparisons below."""
    result = subprocess.run(argv, capture_output=True, text=True, check=False)
    return result.stdout.strip().splitlines()[0] if result.stdout.strip() else ""


def translate_uses(step: dict[str, Any]) -> list[str] | None:
    """The local command for one `uses:` step, or `None` when it has no local equivalent.

    Raises `StepSkipped` (with a reason) for actions that exist only to set the runner up, and
    `RuntimeError` for an action this script does not know - see the module docstring.
    """
    name = action_name(step["uses"])
    inputs = step.get("with") or {}

    if name == "actions/checkout":
        raise StepSkipped("already in the working tree")

    if name == "Swatinem/rust-cache":
        raise StepSkipped("local target/ is the cache")

    if name == "dtolnay/rust-toolchain":
        # Not installed here, but the components CI asks for still have to exist locally, or the
        # `run:` step that needs one fails later with a much less obvious message. A component's
        # name is not the command that runs it - `rustfmt` is invoked as `cargo fmt` - hence the
        # table rather than a guess from the name.
        for component in str(inputs.get("components", "")).replace(",", " ").split():
            check = COMPONENT_CHECKS.get(component, [component, "--version"])
            check_tool(check, f"the {component} component")
        raise StepSkipped("using the local toolchain")

    if name == "actions/setup-node":
        check_tool(["node", "--version"], "the mapping-site JS tests")
        wanted = str(inputs.get("node-version", "")).split(".")[0]
        local = tool_version(["node", "--version"]).lstrip("v").split(".")[0]
        if wanted and local and wanted != local:
            print(
                f"    note: CI uses node {wanted}, this machine has node {local}",
                file=sys.stderr,
            )
        raise StepSkipped("using the local node")

    if name == "astral-sh/setup-uv":
        check_tool(["uv", "--version"], "the Python tests")
        raise StepSkipped("using the local uv")

    if name == "taiki-e/install-action":
        tool = str(inputs.get("tool", "")) or str(step["uses"]).split("@", 1)[1]
        check_tool(["cargo", tool, "--version"], f"the {tool} step")
        raise StepSkipped(f"using the local cargo-{tool}")

    if name == "astral-sh/ruff-action":
        check_tool(["ruff", "--version"], "the Python lint")
        wanted = str(inputs.get("version", ""))
        local = tool_version(["ruff", "--version"]).split()[-1]
        if wanted and local != wanted:
            print(
                f"    note: CI pins ruff {wanted}, this machine has {local} - a lint difference"
                " here may not reproduce in CI",
                file=sys.stderr,
            )
        args = str(inputs.get("args", "")).split()
        src = str(inputs.get("src", "")).split()
        return ["ruff", *args, *src]

    raise RuntimeError(
        f"ci.yml uses `{step['uses']}`, which scripts/ci_local.py does not know how to run"
        " locally.\nAdd it to translate_uses() - deliberately, so a new CI step cannot be"
        " silently skipped here."
    )


def expand(text: str, matrix: dict[str, Any], env: dict[str, str]) -> str:
    """Substitutes the `${{ matrix.x }}` / `${{ env.x }}` forms ci.yml's `run:` steps use.

    Anything else inside `${{ }}` is a hard error rather than a best-effort guess: a silently
    mis-expanded command would run something CI does not, which is worse than not running.
    """
    out: list[str] = []
    rest = text
    while "${{" in rest:
        before, _, after = rest.partition("${{")
        expression, closed, rest = after.partition("}}")
        if not closed:
            raise RuntimeError(f"unterminated ${{{{ in: {text}")
        expression = expression.strip()
        if expression.startswith("matrix."):
            value = matrix.get(expression.removeprefix("matrix."))
            if value is None:
                raise RuntimeError(f"no matrix value for `{expression}` in: {text}")
        elif expression.startswith("env."):
            value = env.get(expression.removeprefix("env."), "")
        else:
            raise RuntimeError(
                f"scripts/ci_local.py cannot evaluate `${{{{ {expression} }}}}` in a run: step."
                " Teach expand() about it rather than letting it run something CI does not."
            )
        out.append(before)
        out.append(str(value))
    out.append(rest)
    return "".join(out)


def matrix_combinations(job: dict[str, Any]) -> list[dict[str, Any]]:
    """Every `strategy.matrix` combination for one job - `[{}]` when it has no matrix."""
    matrix = (job.get("strategy") or {}).get("matrix") or {}
    axes = {key: value for key, value in matrix.items() if isinstance(value, list)}
    if not axes:
        return [{}]
    combinations: list[dict[str, Any]] = [{}]
    for key, values in axes.items():
        combinations = [{**base, key: value} for base in combinations for value in values]
    return combinations


def label(job_id: str, matrix: dict[str, Any]) -> str:
    if not matrix:
        return job_id
    parts = ", ".join(f"{key}={value or 'default'}" for key, value in matrix.items())
    return f"{job_id} ({parts})"


def run_job(job_id: str, job: dict[str, Any], matrix: dict[str, Any], env: dict[str, str]) -> bool:
    heading = label(job_id, matrix)
    print(f"\n\033[1m=== {heading} ===\033[0m", flush=True)
    started = time.monotonic()

    for step in job.get("steps") or []:
        if "uses" in step:
            try:
                argv = translate_uses(step)
            except StepSkipped as skipped:
                print(f"  - {step['uses']}: skipped ({skipped})", flush=True)
                continue
            except RuntimeError as error:
                print(f"  ! {error}", file=sys.stderr, flush=True)
                return False
            if argv is None:
                continue
            printable = " ".join(argv)
            command: list[str] | str = argv
            shell = False
        else:
            command = expand(step["run"], matrix, env)
            printable = str(command).strip()
            shell = True

        print(f"  $ {printable}", flush=True)
        result = subprocess.run(
            command,
            shell=shell,
            cwd=REPO_ROOT,
            env={**os.environ, **env},
            executable="/bin/bash" if shell else None,
            check=False,
        )
        if result.returncode != 0:
            print(
                f"\033[31mFAILED\033[0m {heading}: exit {result.returncode}",
                file=sys.stderr,
                flush=True,
            )
            return False

    print(f"\033[32mok\033[0m {heading} ({time.monotonic() - started:.1f}s)", flush=True)
    return True


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument(
        "--job",
        action="append",
        default=[],
        metavar="ID",
        help="run only this job (repeatable); see --list for the ids",
    )
    parser.add_argument("--list", action="store_true", help="list the jobs and exit")
    parser.add_argument(
        "--keep-going",
        action="store_true",
        help="run every job even after one fails, and report them all at the end",
    )
    args = parser.parse_args()

    workflow = yaml.safe_load(WORKFLOW.read_text())
    env = {key: str(value) for key, value in (workflow.get("env") or {}).items()}
    jobs = workflow["jobs"]

    # Workflow order, not alphabetical: ci.yml already lists the cheap checks before the release
    # build matrix, so following the file puts the fastest failures first for free - and leaves
    # that ordering decision in one place.
    selected = [job_id for job_id in jobs if not args.job or job_id in args.job]
    unknown = [job_id for job_id in args.job if job_id not in jobs]
    if unknown:
        parser.error(f"no such job(s): {', '.join(unknown)} (see --list)")

    if args.list:
        for job_id in jobs:
            for matrix in matrix_combinations(jobs[job_id]):
                print(label(job_id, matrix))
        return 0

    failed: list[str] = []
    for job_id in selected:
        for matrix in matrix_combinations(jobs[job_id]):
            if not run_job(job_id, jobs[job_id], matrix, env):
                failed.append(label(job_id, matrix))
                if not args.keep_going:
                    print(f"\n\033[31mCI failed:\033[0m {failed[0]}", file=sys.stderr)
                    return 1

    if failed:
        print("\n\033[31mCI failed:\033[0m", file=sys.stderr)
        for name in failed:
            print(f"  - {name}", file=sys.stderr)
        return 1

    print("\n\033[32mCI passed\033[0m - every job in .github/workflows/ci.yml")
    return 0


if __name__ == "__main__":
    sys.exit(main())
