"""Python parity for wxyc_etl.pg boundary-safety helpers.

These functions are about PostgreSQL TEXT transport safety, NOT normalization
— they make a string acceptable for storage without changing its meaning.
See `wxyc-etl/src/pg/text.rs` for the Rust contract.
"""

from __future__ import annotations

import pytest
from wxyc_etl import pg


# --- to_pg_text_form -----------------------------------------------------

@pytest.mark.parametrize(
    "input_str,expected",
    [
        ("", ""),
        ("Stereolab", "Stereolab"),
        ("Nilüfer Yanya", "Nilüfer Yanya"),
        ("Csillagrablók", "Csillagrablók"),
        ("Stereo\0lab", "Stereolab"),
        ("\0Autechre", "Autechre"),
        ("Cat Power\0", "Cat Power"),
        ("\0\0\0", ""),
        ("foo\0bar\0baz", "foobarbaz"),
        ("Csillagrabl\0ók", "Csillagrablók"),
        ("line1\tcol\nline2\x0bvt", "line1\tcol\nline2\x0bvt"),
    ],
)
def test_to_pg_text_form(input_str, expected):
    assert pg.to_pg_text_form(input_str) == expected


def test_to_pg_text_form_accepts_none():
    """Mirrors the charter-form Option<&str> -> "" contract."""
    assert pg.to_pg_text_form(None) == ""


def test_to_pg_text_form_idempotent():
    once = pg.to_pg_text_form("foo\0bar\0baz")
    twice = pg.to_pg_text_form(once)
    assert once == twice == "foobarbaz"


# --- batch_to_pg_text_form -----------------------------------------------

def test_batch_to_pg_text_form_matches_singles():
    inputs = ["Stereolab", "Cat\0Power", "Nilüfer Yanya", "\0Autechre\0"]
    expected = [pg.to_pg_text_form(s) for s in inputs]
    assert pg.batch_to_pg_text_form(inputs) == expected


def test_batch_to_pg_text_form_empty():
    assert pg.batch_to_pg_text_form([]) == []


def test_batch_to_pg_text_form_all_clean():
    inputs = ["Juana Molina", "Sessa", "Hermanos Gutiérrez"]
    assert pg.batch_to_pg_text_form(inputs) == inputs
