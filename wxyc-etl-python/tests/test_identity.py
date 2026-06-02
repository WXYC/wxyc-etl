"""PyO3 parity for the cross-cache-identity layered normalizer.

Locks the Python binding against the Rust implementation's behavior. The
authoritative parity matrix lives at
`wxyc-etl/tests/fixtures/identity_normalization_cases.csv` and is exercised
by the Rust integration test `tests/identity_normalization_parity.rs`. The
cases below are a Python-side smoke test that the FFI binding produces the
same results as the Rust unit tests.

Spec: `docs/normalization.md`.
"""

from __future__ import annotations

import pytest
from wxyc_etl import text


@pytest.mark.parametrize(
    "input_str, expected",
    [
        ("Foo (Remastered 2019)", "foo"),
        ("Foo [Live]", "foo"),
        ("Foo (Live) [Remastered]", "foo (live)"),
        ("Foo ()", "foo"),
        ("(Foo)", "(foo)"),
        ("The Beatles", "beatles"),
        ("A Tribe Called Quest", "tribe called quest"),
        ("An Albatross", "albatross"),
        ("Theater", "theater"),
        ("Beatles, The", "beatles"),
        ("Beatles, the Best Of", "beatles, the best of"),
        ("The Beatles (Remastered)", "beatles"),
        ("The Foo Fighters (1995)", "foo fighters"),
        ("The Hermanos Gutiérrez", "hermanos gutierrez"),
        ("", ""),
        ("   ", ""),
    ],
)
def test_identity_match_form(input_str: str, expected: str) -> None:
    assert text.to_identity_match_form(input_str) == expected


def test_identity_match_form_idempotent() -> None:
    for s in [
        "Stereolab",
        "The Beatles",
        "Beatles, The",
        "Foo (Remastered 2019)",
        "Hermanos Gutiérrez",
        "",
    ]:
        once = text.to_identity_match_form(s)
        twice = text.to_identity_match_form(once)
        assert once == twice, f"idempotence broken for {s!r}"


def test_identity_match_form_sigma_collision() -> None:
    final_form = "Στελλάς"
    medial_form = "Στελλάσ"
    assert text.to_identity_match_form(final_form) == text.to_identity_match_form(medial_form)


# --- title variant: same as base today ---


@pytest.mark.parametrize(
    "input_str",
    ["Stereolab", "The Sun Also Rises", "Foo (Live)", "Bar, The", ""],
)
def test_title_variant_matches_base(input_str: str) -> None:
    assert text.to_identity_match_form_title(input_str) == text.to_identity_match_form(input_str)


# --- step 6: punctuation collapse ---


@pytest.mark.parametrize(
    "input_str, expected",
    [
        ("M.I.A.", "m i a"),
        ("R.E.M.", "r e m"),
        ("Godspeed You! Black Emperor", "godspeed you black emperor"),
        ("!!!", ""),
        ("+/-", ""),
        ("10,000 Maniacs", "10 000 maniacs"),
        ("Foo!Bar (Live)", "foo bar"),
        ("The M.I.A.", "m i a"),
        ("Στελλάς", "στελλασ"),
        ("", ""),
    ],
)
def test_with_punctuation(input_str: str, expected: str) -> None:
    assert text.to_identity_match_form_with_punctuation(input_str) == expected


# --- step 8: trailing /N disambiguator ---


@pytest.mark.parametrize(
    "input_str, expected",
    [
        ("John Smith /1", "john smith"),
        ("Various /17", "various"),
        ("Track 1/12", "track 1/12"),
        ("Side A/B", "side a/b"),
        ("The John Smith /3 (1995)", "john smith"),
        ("Stereolab", "stereolab"),
        ("", ""),
    ],
)
def test_with_disambiguator_strip(input_str: str, expected: str) -> None:
    assert text.to_identity_match_form_with_disambiguator_strip(input_str) == expected


# --- standalone article stripper (PyO3-exposed; wxyc-etl#133) ---


@pytest.mark.parametrize(
    "input_str, expected",
    [
        ("the beatles", "beatles"),
        ("a tribe called quest", "tribe called quest"),
        ("an albatross", "albatross"),
        ("the", ""),
        ("a", ""),
        ("an", ""),
        ("theater", "theater"),
        ("thee silver mt zion", "thee silver mt zion"),
        ("animal", "animal"),
        ("stereolab", "stereolab"),
        ("the the", "the"),
        ("the  beatles", "beatles"),
        # Unicode whitespace after the article — matches Python `\s+` semantics.
        ("the beatles", "beatles"),
        ("the beatles", "beatles"),
        ("the beatles", "beatles"),
        ("the beatles", "beatles"),
        # Input contract: lowercased + trimmed. Uppercased articles are a no-op.
        ("The Beatles", "The Beatles"),
        ("", ""),
    ],
)
def test_strip_leading_article(input_str: str, expected: str) -> None:
    assert text.strip_leading_article(input_str) == expected
