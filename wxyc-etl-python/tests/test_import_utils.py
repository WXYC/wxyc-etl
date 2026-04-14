"""Tests for wxyc_etl.import_utils bindings (DedupSet)."""

from wxyc_etl.import_utils import DedupSet


def test_dedup_set_basic():
    ds = DedupSet()
    assert len(ds) == 0

    # First insert returns True (new key)
    assert ds.add(["artist_a", "title_1"]) is True
    assert len(ds) == 1

    # Duplicate returns False
    assert ds.add(["artist_a", "title_1"]) is False
    assert len(ds) == 1

    # Different key returns True
    assert ds.add(["artist_b", "title_2"]) is True
    assert len(ds) == 2


def test_dedup_set_contains():
    ds = DedupSet()
    ds.add(["artist_a", "title_1"])

    assert ["artist_a", "title_1"] in ds
    assert ["artist_b", "title_2"] not in ds


def test_dedup_set_none_handling():
    """None values in keys should be treated as empty strings."""
    ds = DedupSet()
    assert ds.add([None, "title"]) is True
    assert ds.add(["", "title"]) is False  # None == ""
    assert len(ds) == 1


def test_dedup_set_empty_key():
    ds = DedupSet()
    assert ds.add([]) is True
    assert ds.add([]) is False
    assert len(ds) == 1
