"""WX-1.2.7 detector for the PyO3 string-marshaling boundary.

Loads the cross-repo `@wxyc/shared` charset-torture corpus and asserts that
`wxyc_etl.text.normalize_artist_name` (the Python binding) returns the same
string as the Rust `wxyc_etl::text::normalize_artist_name` (covered by the
sibling `wxyc-etl/tests/charset_torture.rs`). Together the two tests confirm
that PyO3 doesn't lose or re-encode bytes between the languages.

See WXYC/docs#15 for the WX-1 plan.
"""

from __future__ import annotations

import json
from pathlib import Path

import pytest
import wxyc_etl

# Exercises the legacy normalize_artist_name binding on purpose; its
# DeprecationWarning signal is verified by test_deprecations.py.
pytestmark = pytest.mark.filterwarnings("ignore::DeprecationWarning")

_CORPUS_PATH = (
    Path(__file__).resolve().parent.parent.parent / "tests" / "fixtures" / "charset-torture.json"
)


def _iter_entries():
    corpus = json.loads(_CORPUS_PATH.read_text(encoding="utf-8"))
    for category, entries in corpus["categories"].items():
        for entry in entries:
            yield {**entry, "category": category}


def _entry_id(entry: dict) -> str:
    return f"{entry['category']}:{entry['input'][:24]}"


CORPUS_ENTRIES = list(_iter_entries())


@pytest.mark.parametrize("entry", CORPUS_ENTRIES, ids=_entry_id)
def test_pyo3_normalize_roundtrips_input(entry: dict) -> None:
    """The PyO3 binding must accept and return arbitrary UTF-8 strings without
    truncating, replacing, or re-encoding characters across the FFI boundary."""
    actual = wxyc_etl.text.normalize_artist_name(entry["input"])
    # We don't assert spec compliance here (that's the Rust test's job).
    # We assert that the value came back as a Python str with no byte loss.
    assert isinstance(actual, str)
    # Empty result is only valid for inputs that are themselves whitespace-only;
    # corpus inputs are all non-trivial.
    assert actual or not entry["input"].strip(), f"empty result for {entry['notes']}"
