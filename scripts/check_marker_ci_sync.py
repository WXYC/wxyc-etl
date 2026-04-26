#!/usr/bin/env python3
"""Verify pytest marker scheme stays in sync with CI workflows.

The bug pattern this guards against (originally surfaced by WXYC/discogs-etl#103):
a repo declares pytest markers (e.g. `postgres`, `integration`, `e2e`), addopts
deselects them by default, and the CI workflow either has no job that
re-selects them or invokes pytest without `-m` overriding addopts. Marked tests
exist, contributors assume CI runs them, CI silently deselects them. The bug is
invisible because pytest reports "N deselected" rather than "0 ran".

This script reads ``pyproject.toml`` (markers + addopts) and every
``.github/workflows/*.yml`` (every step that invokes pytest), and fails when a
marker actually used by a test in this repo is excluded by addopts AND not
explicitly re-selected by any CI invocation.

Opt-out: place a comment anywhere in pyproject.toml of the form
``# ci-sync-skip: <marker> reason: <text>`` and the script will skip that
marker (e.g. for markers that are intentionally manual-only).

Exit codes:
  0 — all used markers are reachable from CI (or opted out)
  1 — at least one used marker is silently deselected by CI
  2 — script could not run (missing files, parse error)
"""

from __future__ import annotations

import argparse
import re
import sys
import tomllib
from collections.abc import Iterable
from pathlib import Path

import yaml

# Markers built into pytest or common plugins; never flag these as gaps.
BUILTIN_MARKERS = frozenset({
    "asyncio",
    "filterwarnings",
    "parametrize",
    "skip",
    "skipif",
    "usefixtures",
    "xfail",
})


def find_used_markers(tests_dir: Path) -> set[str]:
    """Return every marker referenced via ``@pytest.mark.X`` or ``pytest.mark.X``."""
    if not tests_dir.exists():
        return set()
    pattern = re.compile(r"pytest\.mark\.(\w+)")
    markers: set[str] = set()
    for py_file in tests_dir.rglob("*.py"):
        try:
            text = py_file.read_text(encoding="utf-8")
        except (OSError, UnicodeDecodeError):
            continue
        markers.update(pattern.findall(text))
    return markers - BUILTIN_MARKERS


def parse_pyproject(repo: Path) -> tuple[set[str], set[str], dict[str, str]]:
    """Return (declared_markers, addopts_excluded_markers, opt_outs)."""
    pyproject = repo / "pyproject.toml"
    if not pyproject.exists():
        return set(), set(), {}
    with pyproject.open("rb") as f:
        data = tomllib.load(f)
    pytest_cfg = data.get("tool", {}).get("pytest", {}).get("ini_options", {})

    declared: set[str] = set()
    for entry in pytest_cfg.get("markers", []):
        # Each entry is "name: description"; take everything before the colon.
        name = entry.split(":", 1)[0].strip()
        if name:
            declared.add(name)

    addopts = pytest_cfg.get("addopts", "")
    if isinstance(addopts, list):
        addopts = " ".join(addopts)
    excluded = set(re.findall(r"\bnot\s+(\w+)", addopts))

    opt_outs: dict[str, str] = {}
    raw = pyproject.read_text(encoding="utf-8")
    for m in re.finditer(r"#\s*ci-sync-skip:\s*(\w+)(?:\s+reason:\s*([^\n]+))?", raw):
        opt_outs[m.group(1)] = (m.group(2) or "").strip()

    return declared, excluded, opt_outs


def _parse_marker_expression(expr: str) -> set[str]:
    """Extract positive marker terms from a pytest -m expression.

    ``"postgres"`` -> {"postgres"}; ``"not slow"`` -> set();
    ``"integration or postgres"`` -> {"integration", "postgres"};
    ``"postgres and not slow"`` -> {"postgres"}.
    """
    tokens = re.findall(r"\(|\)|\bnot\b|\band\b|\bor\b|\b\w+\b", expr)
    positives: set[str] = set()
    i = 0
    while i < len(tokens):
        tok = tokens[i]
        if tok == "not":
            i += 2  # skip the marker that follows
            continue
        if tok in {"and", "or", "(", ")"}:
            i += 1
            continue
        positives.add(tok)
        i += 1
    return positives


def _iter_workflow_steps(workflows_dir: Path) -> Iterable[tuple[Path, str, str]]:
    """Yield (workflow_path, job_name, run_script) for each step that runs pytest."""
    if not workflows_dir.exists():
        return
    for wf in sorted(workflows_dir.glob("*.yml")):
        try:
            data = yaml.safe_load(wf.read_text(encoding="utf-8"))
        except (OSError, yaml.YAMLError):
            continue
        if not isinstance(data, dict):
            continue
        for job_name, job in (data.get("jobs") or {}).items():
            if not isinstance(job, dict):
                continue
            for step in job.get("steps") or []:
                if not isinstance(step, dict):
                    continue
                run = step.get("run", "")
                if isinstance(run, str) and "pytest" in run:
                    yield wf, job_name, run


def find_ci_selected_markers(workflows_dir: Path) -> tuple[set[str], bool]:
    """Return (markers_explicitly_selected_by_some_pytest_job, any_pytest_runs_without_m).

    A marker is "selected" if some CI pytest invocation passes ``-m "..."`` whose
    expression includes the marker as a positive term. ``any_pytest_runs_without_m``
    is True when at least one pytest step omits ``-m`` entirely (so it inherits
    addopts and would deselect addopts-excluded markers).
    """
    selected: set[str] = set()
    any_without_m = False
    m_arg_re = re.compile(r"-m[ =]+(?:\"([^\"]+)\"|'([^']+)')")
    for _, _, run in _iter_workflow_steps(workflows_dir):
        # Only count -m args attached to a pytest invocation. A single run
        # script may invoke pytest multiple times; analyse each line.
        for line in run.splitlines():
            if "pytest" not in line:
                continue
            args = list(m_arg_re.finditer(line))
            if not args:
                any_without_m = True
                continue
            for a in args:
                expr = a.group(1) or a.group(2) or ""
                selected.update(_parse_marker_expression(expr))
    return selected, any_without_m


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--repo-path",
        type=Path,
        default=Path.cwd(),
        help="Repo root (default: cwd)",
    )
    parser.add_argument(
        "--tests-dir",
        type=Path,
        default=None,
        help="Override tests dir (default: <repo>/tests)",
    )
    parser.add_argument(
        "--workflows-dir",
        type=Path,
        default=None,
        help="Override CI workflows dir (default: <repo>/.github/workflows)",
    )
    args = parser.parse_args(argv)

    repo = args.repo_path.resolve()
    workflows_dir = (
        args.workflows_dir.resolve() if args.workflows_dir else repo / ".github" / "workflows"
    )
    tests_dir = args.tests_dir.resolve() if args.tests_dir else repo / "tests"

    if not workflows_dir.exists():
        print(f"FAIL: no .github/workflows directory at {workflows_dir}", file=sys.stderr)
        return 2

    used = find_used_markers(tests_dir)
    declared, excluded, opt_outs = parse_pyproject(repo)
    selected, any_without_m = find_ci_selected_markers(workflows_dir)

    print(f"Repo:                      {repo}")
    print(f"Tests dir:                 {tests_dir}")
    print(f"Markers used by tests:     {sorted(used) or '(none)'}")
    print(f"Markers declared in toml:  {sorted(declared) or '(none)'}")
    print(f"Excluded by addopts:       {sorted(excluded) or '(none)'}")
    print(f"Selected by CI -m args:    {sorted(selected) or '(none)'}")
    print(f"Some CI step runs pytest without -m: {any_without_m}")
    if opt_outs:
        print(f"Opt-outs (ci-sync-skip):   {opt_outs}")
    print()

    gaps: list[str] = []
    for marker in sorted(used):
        if marker in opt_outs:
            print(f"  SKIP {marker}: opt-out -- {opt_outs[marker] or '(no reason given)'}")
            continue
        deselected_by_default = marker in excluded
        reachable = (not deselected_by_default) or (marker in selected)
        if reachable:
            print(f"  OK   {marker}")
        else:
            gaps.append(marker)
            print(f"  GAP  {marker}: excluded by addopts, no CI -m argument re-selects it")

    orphans = sorted(declared - used)
    if orphans:
        print()
        print(f"Note (warning, not gap): markers declared but never used by any test: {orphans}")

    print()
    if gaps:
        print(f"FAIL: {len(gaps)} marker(s) silently deselected by CI: {gaps}", file=sys.stderr)
        print(
            "Fix: either add a CI job that runs `pytest -m \"<marker>\" ...`, "
            "remove the marker from addopts, or document an intentional opt-out "
            "with a `# ci-sync-skip: <marker> reason: <text>` comment in pyproject.toml.",
            file=sys.stderr,
        )
        return 1

    print("PASS: every used marker is reachable from CI")
    return 0


if __name__ == "__main__":
    sys.exit(main())
