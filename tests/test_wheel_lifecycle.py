"""Wheel lifecycle test: build -> install -> import -> smoke test.

Verifies that the wxyc-etl PyO3 wheel can be built with maturin, installed
in a fresh virtualenv, and that every submodule imports and functions correctly.
This catches build configuration issues that unit tests miss:
- Missing submodule registration in lib.rs
- Import path mismatches between Rust module names and Python import paths
- Missing #[pyfunction] or #[pyclass] annotations
- Broken FFI signatures (argument types, return types)

Can be run standalone or as a CI step after `maturin build`.

Usage:
    # Build and test in one step:
    python tests/test_wheel_lifecycle.py

    # Or in CI:
    maturin build --release
    pip install target/wheels/wxyc_etl-*.whl
    python tests/test_wheel_lifecycle.py --skip-build
"""

from __future__ import annotations

import argparse
import glob
import os
import subprocess
import sys
import tempfile
import venv
from pathlib import Path

REPO_ROOT = Path(__file__).parent.parent
PYTHON_CRATE = REPO_ROOT / "wxyc-etl-python"


def find_wheel(target_dir: Path) -> Path | None:
    """Find the most recently built wheel in target/wheels/."""
    wheels = sorted(
        glob.glob(str(target_dir / "wheels" / "wxyc_etl-*.whl")),
        key=os.path.getmtime,
        reverse=True,
    )
    return Path(wheels[0]) if wheels else None


def build_wheel() -> Path:
    """Build the wheel using maturin."""
    print("Building wheel with maturin...")
    result = subprocess.run(
        ["maturin", "build", "--release", "--manifest-path", str(PYTHON_CRATE / "Cargo.toml")],
        capture_output=True,
        text=True,
        cwd=str(REPO_ROOT),
        timeout=300,
    )
    if result.returncode != 0:
        print("STDOUT:", result.stdout)
        print("STDERR:", result.stderr)
        raise RuntimeError(f"maturin build failed (exit {result.returncode}):\n{result.stderr}")

    wheel = find_wheel(REPO_ROOT / "target")
    if wheel is None:
        raise RuntimeError("maturin build succeeded but no wheel found in target/wheels/")
    print(f"Built wheel: {wheel}")
    return wheel


def create_venv_and_install(wheel: Path) -> Path:
    """Create a fresh virtualenv and install the wheel."""
    venv_dir = Path(tempfile.mkdtemp(prefix="wxyc_etl_venv_"))
    print(f"Creating virtualenv at {venv_dir}...")
    venv.create(str(venv_dir), with_pip=True)

    if sys.platform == "win32":
        pip = venv_dir / "Scripts" / "pip"
        python = venv_dir / "Scripts" / "python"
    else:
        pip = venv_dir / "bin" / "pip"
        python = venv_dir / "bin" / "python"

    # Install the wheel
    print(f"Installing {wheel.name}...")
    result = subprocess.run(
        [str(pip), "install", str(wheel)],
        capture_output=True,
        text=True,
        timeout=60,
    )
    if result.returncode != 0:
        raise RuntimeError(f"pip install failed:\n{result.stderr}")

    return python


def run_smoke_tests(python: Path) -> bool:
    """Run import and smoke tests using the venv's Python interpreter."""
    smoke_test_code = '''
import sys
import json

passed = 0
failed = 0
failures = []

def check(name, expr_result, expected=None):
    """Record a check result. If expected is given, assert equality."""
    global passed, failed
    try:
        if expected is not None:
            assert expr_result == expected, f"got {expr_result!r}, expected {expected!r}"
        passed += 1
    except Exception as e:
        failed += 1
        failures.append(f"  {name}: {e}")

# ============================================================
# 1. Import all submodules
# ============================================================

import wxyc_etl
check("import wxyc_etl", True)

from wxyc_etl.text import (
    normalize_artist_name, is_compilation_artist, split_artist_name,
    split_artist_name_contextual, strip_diacritics, batch_normalize,
)
check("import wxyc_etl.text (all exports)", True)

from wxyc_etl.fuzzy import batch_fuzzy_resolve, jaro_winkler_similarity
check("import wxyc_etl.fuzzy (all exports)", True)

from wxyc_etl.parser import load_table_rows, iter_table_rows, parse_sql_values
check("import wxyc_etl.parser (all exports)", True)

from wxyc_etl.state import PipelineState
check("import wxyc_etl.state (PipelineState)", True)

from wxyc_etl.import_utils import DedupSet
check("import wxyc_etl.import_utils (DedupSet)", True)

from wxyc_etl.schema import (
    RELEASE_TABLE, RELEASE_ARTIST_TABLE, RELEASE_LABEL_TABLE,
    RELEASE_STYLE_TABLE, RELEASE_TRACK_TABLE, RELEASE_TRACK_ARTIST_TABLE,
    discogs_tables, discogs_release_columns, library_ddl, library_columns,
    entity_identity_ddl, entity_identity_columns,
)
check("import wxyc_etl.schema (all exports)", True)

# ============================================================
# 2. Smoke test: call one function from each submodule
# ============================================================

# -- text module --

check("normalize_artist_name('Stereolab')",
      normalize_artist_name("Stereolab"), "stereolab")

check("normalize_artist_name(None)",
      normalize_artist_name(None), "")

check("is_compilation_artist('Various Artists')",
      is_compilation_artist("Various Artists"), True)

check("is_compilation_artist('Juana Molina')",
      is_compilation_artist("Juana Molina"), False)

check("strip_diacritics('Bjork')",
      strip_diacritics("Bjork"), "Bjork")

check("batch_normalize(['Cat Power', 'Sessa'])",
      batch_normalize(["Cat Power", "Sessa"]), ["cat power", "sessa"])

check("split_artist_name no split without context",
      split_artist_name("Duke Ellington & John Coltrane"), None)

ctx_result = split_artist_name_contextual(
    "Duke Ellington & John Coltrane",
    {"duke ellington", "john coltrane"}
)
check("split_artist_name_contextual splits with known artists",
      ctx_result is not None and "Duke Ellington" in ctx_result, True)

# -- fuzzy module --

check("jaro_winkler_similarity('cat', 'cat')",
      jaro_winkler_similarity("cat", "cat"), 1.0)

jw = jaro_winkler_similarity("Stereolab", "Stereolabe")
check("jaro_winkler_similarity typo > 0.9",
      jw > 0.9, True)

resolve_result = batch_fuzzy_resolve(
    ["Stereolabe"], ["Stereolab", "Cat Power"], 0.85
)
check("batch_fuzzy_resolve resolves typo",
      resolve_result, ["Stereolab"])

# -- parser module --

rows = parse_sql_values("(1,'Stereolab'),(2,'Cat Power')")
check("parse_sql_values row count", len(rows), 2)
check("parse_sql_values first row", (rows[0][0], rows[0][1]), (1, "Stereolab"))
check("parse_sql_values second row", (rows[1][0], rows[1][1]), (2, "Cat Power"))

# -- state module --

state = PipelineState("postgresql:///test", "/tmp/csv", ["step1", "step2"])
check("PipelineState initial status", state.is_completed("step1"), False)

state.mark_completed("step1")
check("PipelineState after mark_completed", state.is_completed("step1"), True)

check("PipelineState step_status completed",
      state.step_status("step1"), "completed")
check("PipelineState step_status pending",
      state.step_status("step2"), "pending")

# -- import_utils module --

ds = DedupSet()
first_add = ds.add(["Stereolab", "Aluminum Tunes"])
check("DedupSet.add new key returns True", first_add, True)

second_add = ds.add(["Stereolab", "Aluminum Tunes"])
check("DedupSet.add duplicate returns False", second_add, False)

check("DedupSet.__contains__ existing key",
      ["Stereolab", "Aluminum Tunes"] in ds, True)
check("DedupSet.__contains__ missing key",
      ["Cat Power", "Moon Pix"] in ds, False)
check("DedupSet.__len__", len(ds), 1)

# -- schema module --

check("RELEASE_TABLE is a non-empty string",
      isinstance(RELEASE_TABLE, str) and len(RELEASE_TABLE) > 0, True)
check("RELEASE_ARTIST_TABLE is a string",
      isinstance(RELEASE_ARTIST_TABLE, str), True)
check("RELEASE_LABEL_TABLE is a string",
      isinstance(RELEASE_LABEL_TABLE, str), True)
check("RELEASE_STYLE_TABLE is a string",
      isinstance(RELEASE_STYLE_TABLE, str), True)
check("RELEASE_TRACK_TABLE is a string",
      isinstance(RELEASE_TRACK_TABLE, str), True)
check("RELEASE_TRACK_ARTIST_TABLE is a string",
      isinstance(RELEASE_TRACK_ARTIST_TABLE, str), True)

tables = discogs_tables()
check("discogs_tables() returns non-empty list",
      isinstance(tables, list) and len(tables) > 0, True)

columns = discogs_release_columns()
check("discogs_release_columns() contains 'id'",
      isinstance(columns, list) and "id" in columns, True)

ddl = library_ddl()
check("library_ddl() contains CREATE TABLE",
      "CREATE TABLE" in ddl, True)

cols = library_columns()
check("library_columns() returns non-empty list",
      isinstance(cols, list) and len(cols) > 0, True)

ddl = entity_identity_ddl()
check("entity_identity_ddl() contains entity.identity",
      "entity.identity" in ddl, True)

cols = entity_identity_columns()
check("entity_identity_columns() contains library_name",
      isinstance(cols, list) and "library_name" in cols, True)

# ============================================================
# 3. Report
# ============================================================

print(f"Passed: {passed}, Failed: {failed}")
if failures:
    print("Failures:")
    for f in failures:
        print(f)
    sys.exit(1)
else:
    print("All checks passed.")
    sys.exit(0)
'''

    print("Running smoke tests...")
    result = subprocess.run(
        [str(python), "-c", smoke_test_code],
        capture_output=True,
        text=True,
        timeout=60,
    )

    print(result.stdout)
    if result.stderr:
        print("STDERR:", result.stderr)

    return result.returncode == 0


def main():
    parser = argparse.ArgumentParser(description="wxyc-etl wheel lifecycle test")
    parser.add_argument(
        "--skip-build",
        action="store_true",
        help="Skip building; use an already-built wheel from target/wheels/",
    )
    args = parser.parse_args()

    if args.skip_build:
        wheel = find_wheel(REPO_ROOT / "target")
        if wheel is None:
            print("ERROR: No wheel found in target/wheels/. Run maturin build first.")
            sys.exit(1)
        print(f"Using existing wheel: {wheel}")
    else:
        wheel = build_wheel()

    python = create_venv_and_install(wheel)

    if run_smoke_tests(python):
        print("\nWheel lifecycle test PASSED")
        sys.exit(0)
    else:
        print("\nWheel lifecycle test FAILED")
        sys.exit(1)


if __name__ == "__main__":
    main()
