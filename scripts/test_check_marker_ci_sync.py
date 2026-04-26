"""Unit tests for ``check_marker_ci_sync``."""

from __future__ import annotations

import textwrap
from pathlib import Path

import pytest

from check_marker_ci_sync import (
    _parse_marker_expression,
    find_ci_selected_markers,
    find_used_markers,
    main,
    parse_pyproject,
)


@pytest.mark.parametrize(
    "expr,expected",
    [
        ("postgres", {"postgres"}),
        ("not slow", set()),
        ("integration or postgres", {"integration", "postgres"}),
        ("postgres and not slow", {"postgres"}),
        ("not slow and not integration", set()),
        ("(postgres or e2e) and not slow", {"postgres", "e2e"}),
        ("", set()),
    ],
)
def test_parse_marker_expression(expr: str, expected: set[str]) -> None:
    assert _parse_marker_expression(expr) == expected


def _make_repo(tmp_path: Path, *, pyproject: str, workflow: str, tests: dict[str, str]) -> Path:
    (tmp_path / "pyproject.toml").write_text(textwrap.dedent(pyproject))
    wf_dir = tmp_path / ".github" / "workflows"
    wf_dir.mkdir(parents=True)
    (wf_dir / "ci.yml").write_text(textwrap.dedent(workflow))
    tests_dir = tmp_path / "tests"
    tests_dir.mkdir()
    for name, body in tests.items():
        (tests_dir / name).write_text(textwrap.dedent(body))
    return tmp_path


def test_pass_when_marker_excluded_but_re_selected_by_ci(tmp_path: Path) -> None:
    repo = _make_repo(
        tmp_path,
        pyproject="""
            [tool.pytest.ini_options]
            markers = ["postgres: pg-backed tests"]
            addopts = "-m 'not postgres'"
        """,
        workflow="""
            jobs:
              test-postgres:
                steps:
                  - run: pytest -m "postgres" tests/
        """,
        tests={"test_x.py": "import pytest\n@pytest.mark.postgres\ndef test_a(): pass\n"},
    )
    assert main(["--repo-path", str(repo)]) == 0


def test_fail_when_marker_excluded_and_no_ci_re_selects(tmp_path: Path) -> None:
    repo = _make_repo(
        tmp_path,
        pyproject="""
            [tool.pytest.ini_options]
            markers = ["postgres: pg tests"]
            addopts = "-m 'not postgres'"
        """,
        workflow="""
            jobs:
              test:
                steps:
                  - run: pytest -v tests/
        """,
        tests={"test_x.py": "import pytest\n@pytest.mark.postgres\ndef test_a(): pass\n"},
    )
    assert main(["--repo-path", str(repo)]) == 1


def test_pass_when_addopts_does_not_exclude_marker(tmp_path: Path) -> None:
    repo = _make_repo(
        tmp_path,
        pyproject="""
            [tool.pytest.ini_options]
            markers = ["unit: fast unit tests"]
        """,
        workflow="""
            jobs:
              test:
                steps:
                  - run: pytest -v tests/
        """,
        tests={"test_x.py": "import pytest\n@pytest.mark.unit\ndef test_a(): pass\n"},
    )
    assert main(["--repo-path", str(repo)]) == 0


def test_opt_out_skips_gap(tmp_path: Path) -> None:
    repo = _make_repo(
        tmp_path,
        pyproject="""
            # ci-sync-skip: e2e reason: manual run only
            [tool.pytest.ini_options]
            markers = ["e2e: end-to-end (manual)"]
            addopts = "-m 'not e2e'"
        """,
        workflow="""
            jobs:
              test:
                steps:
                  - run: pytest -v tests/
        """,
        tests={"test_x.py": "import pytest\n@pytest.mark.e2e\ndef test_a(): pass\n"},
    )
    assert main(["--repo-path", str(repo)]) == 0


def test_orphan_markers_are_not_a_gap(tmp_path: Path) -> None:
    """A marker declared in pyproject but never used by any test is a soft warning, not a failure."""
    repo = _make_repo(
        tmp_path,
        pyproject="""
            [tool.pytest.ini_options]
            markers = [
                "unit: fast unit tests",
                "parity: declared but unused",
            ]
            addopts = "-m 'not parity'"
        """,
        workflow="""
            jobs:
              test:
                steps:
                  - run: pytest -v tests/
        """,
        tests={"test_x.py": "import pytest\n@pytest.mark.unit\ndef test_a(): pass\n"},
    )
    assert main(["--repo-path", str(repo)]) == 0


def test_compound_addopts_excludes_multiple_markers(tmp_path: Path) -> None:
    repo = _make_repo(
        tmp_path,
        pyproject="""
            [tool.pytest.ini_options]
            markers = ["integration: int", "postgres: pg", "e2e: end"]
            addopts = "-m 'not integration and not postgres and not e2e'"
        """,
        workflow="""
            jobs:
              integration:
                steps:
                  - run: pytest -m "integration" tests/
              postgres:
                steps:
                  - run: pytest -m "postgres" tests/
        """,
        tests={
            "test_x.py": (
                "import pytest\n"
                "@pytest.mark.integration\n"
                "def test_a(): pass\n"
                "@pytest.mark.postgres\n"
                "def test_b(): pass\n"
                "@pytest.mark.e2e\n"
                "def test_c(): pass\n"
            ),
        },
    )
    # e2e should be flagged: declared+used+excluded but no CI job selects it.
    assert main(["--repo-path", str(repo)]) == 1


def test_pytestmark_module_marker_detected(tmp_path: Path) -> None:
    """The module-level ``pytestmark = pytest.mark.X`` form must be detected."""
    repo = _make_repo(
        tmp_path,
        pyproject="""
            [tool.pytest.ini_options]
            markers = ["postgres: pg"]
            addopts = "-m 'not postgres'"
        """,
        workflow="""
            jobs:
              test:
                steps:
                  - run: pytest -v tests/
        """,
        tests={"test_x.py": "import pytest\npytestmark = pytest.mark.postgres\ndef test_a(): pass\n"},
    )
    assert main(["--repo-path", str(repo)]) == 1  # postgres used via pytestmark, no CI selects


def test_addopts_as_list(tmp_path: Path) -> None:
    """addopts may be a list of args; the script should handle that form."""
    pyproject = """
        [tool.pytest.ini_options]
        markers = ["postgres: pg"]
        addopts = ["-v", "-m", "not postgres"]
    """
    (tmp_path / "pyproject.toml").write_text(textwrap.dedent(pyproject))
    declared, excluded, _ = parse_pyproject(tmp_path)
    assert excluded == {"postgres"}


def test_no_workflows_dir_returns_2(tmp_path: Path) -> None:
    (tmp_path / "pyproject.toml").write_text("[tool.pytest.ini_options]\nmarkers=[]\n")
    assert main(["--repo-path", str(tmp_path)]) == 2


def test_ci_with_pytest_in_compound_command(tmp_path: Path) -> None:
    """``pytest`` invoked alongside other commands in one run: block."""
    repo = _make_repo(
        tmp_path,
        pyproject="""
            [tool.pytest.ini_options]
            markers = ["postgres: pg"]
            addopts = "-m 'not postgres'"
        """,
        workflow="""
            jobs:
              test:
                steps:
                  - run: |
                      pip install -e .
                      pytest -m "postgres" tests/
        """,
        tests={"test_x.py": "import pytest\n@pytest.mark.postgres\ndef test_a(): pass\n"},
    )
    assert main(["--repo-path", str(repo)]) == 0


def test_finds_ci_selected_markers_handles_quotes(tmp_path: Path) -> None:
    wf_dir = tmp_path / ".github" / "workflows"
    wf_dir.mkdir(parents=True)
    (wf_dir / "ci.yml").write_text(textwrap.dedent("""
        jobs:
          a:
            steps:
              - run: pytest -m "postgres or integration" tests/
          b:
            steps:
              - run: pytest -m 'e2e' tests/
          c:
            steps:
              - run: pytest -v tests/
    """))
    selected, any_without_m = find_ci_selected_markers(wf_dir)
    assert selected == {"postgres", "integration", "e2e"}
    assert any_without_m is True


def test_find_used_markers_filters_builtins(tmp_path: Path) -> None:
    tests_dir = tmp_path / "tests"
    tests_dir.mkdir()
    (tests_dir / "test_x.py").write_text(
        "import pytest\n"
        "@pytest.mark.parametrize('x', [1])\n"
        "@pytest.mark.skip\n"
        "@pytest.mark.postgres\n"
        "def test_a(x): pass\n"
    )
    used = find_used_markers(tests_dir)
    assert used == {"postgres"}
