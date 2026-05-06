"""WX-2.2.4 Python parity tests for the WX-2 Normalizer Charter forms.

For each WX-1 charset-torture corpus entry that defines an
`expected_storage_form` / `expected_match_form` / `expected_ascii_form`,
assert that the PyO3 binding returns the same string as the Rust
implementation (which is covered by `wxyc-etl/tests/forms_*.rs`).

This is the cross-FFI parity contract called out in WX-2.2.4: the same
input must produce the same output regardless of which language calls
the function.

Also covers the `batch_to_*_form` variants — the batched binding must
return the same list as `[to_*_form(s) for s in inputs]`.
"""

from __future__ import annotations

import json
from pathlib import Path

import pytest
from wxyc_etl import text

_CORPUS_PATH = (
    Path(__file__).resolve().parent.parent.parent / "tests" / "fixtures" / "charset-torture.json"
)


def _load_corpus():
    return json.loads(_CORPUS_PATH.read_text(encoding="utf-8"))


def _entries_with(field: str):
    """Yield (category, entry) for every corpus entry where `field` is non-null."""
    for category, entries in _load_corpus()["categories"].items():
        for entry in entries:
            if entry.get(field) is not None:
                yield category, entry


def _entry_id(pair) -> str:
    category, entry = pair
    return f"{category}:{entry['input'][:24]}"


STORAGE_ENTRIES = list(_entries_with("expected_storage_form"))
MATCH_ENTRIES = list(_entries_with("expected_match_form"))
ASCII_ENTRIES = list(_entries_with("expected_ascii_form"))


@pytest.mark.parametrize("pair", STORAGE_ENTRIES, ids=_entry_id)
def test_to_storage_form_matches_corpus(pair) -> None:
    _, entry = pair
    assert text.to_storage_form(entry["input"]) == entry["expected_storage_form"], entry["notes"]


@pytest.mark.parametrize("pair", MATCH_ENTRIES, ids=_entry_id)
def test_to_match_form_matches_corpus(pair) -> None:
    _, entry = pair
    assert text.to_match_form(entry["input"]) == entry["expected_match_form"], entry["notes"]


@pytest.mark.parametrize("pair", ASCII_ENTRIES, ids=_entry_id)
def test_to_ascii_form_matches_corpus(pair) -> None:
    _, entry = pair
    assert text.to_ascii_form(entry["input"]) == entry["expected_ascii_form"], entry["notes"]


# --- batch variants ---


def _all_inputs() -> list[str]:
    return [entry["input"] for _, entries in _load_corpus()["categories"].items() for entry in entries]


def test_batch_to_storage_form_matches_singles() -> None:
    inputs = _all_inputs()
    assert text.batch_to_storage_form(inputs) == [text.to_storage_form(s) for s in inputs]


def test_batch_to_match_form_matches_singles() -> None:
    inputs = _all_inputs()
    assert text.batch_to_match_form(inputs) == [text.to_match_form(s) for s in inputs]


def test_batch_to_ascii_form_matches_singles() -> None:
    inputs = _all_inputs()
    assert text.batch_to_ascii_form(inputs) == [text.to_ascii_form(s) for s in inputs]


def test_batch_to_storage_form_empty() -> None:
    assert text.batch_to_storage_form([]) == []


def test_batch_to_match_form_empty() -> None:
    assert text.batch_to_match_form([]) == []


def test_batch_to_ascii_form_empty() -> None:
    assert text.batch_to_ascii_form([]) == []
