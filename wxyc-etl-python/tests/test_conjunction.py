"""PyO3 parity for the conjunction fold (wxyc-etl#147).

Locks the Python binding against the Rust implementation's decision table
(`wxyc-etl/src/text/conjunction.rs`), which reconciles three overlapping
`library-metadata-lookup` "&"-to-"and" implementations. See that module's
doc comment for the full rationale behind each row below.
"""

from __future__ import annotations

import pytest
from wxyc_etl import text


@pytest.mark.parametrize(
    "input_str, expected",
    [
        # Spaced ampersand: all three LML variants agree — fold.
        ("Duke Ellington & John Coltrane", "Duke Ellington and John Coltrane"),
        # Glued ampersand inside a real genre tag. DECISION: whitespace is
        # required on both sides to fold, so typographic/abbreviation
        # ampersands are left alone (see the Rust module docs).
        ("R&B", "R&B"),
        ("A&E", "A&E"),
        ("AT&T", "AT&T"),
        ("M&M", "M&M"),
        # One-sided padding does not fold.
        ("Foo &Bar", "Foo &Bar"),
        ("Foo& Bar", "Foo& Bar"),
        # Multiple interior spaces collapse to a single " and ".
        ("Duke Ellington  &  John Coltrane", "Duke Ellington and John Coltrane"),
        # Multiple conjunctions in one string, spaced apart.
        ("A & B & C", "A and B and C"),
        # Known divergence from `identity/normalize.py` (see the Rust
        # module docs): two ampersands sharing a single whitespace run fold
        # independently here ("A and and B"), whereas Python's
        # non-overlapping regex sub consumes the shared space in its first
        # match and yields "A and & B". Not a realistic artist credit
        # shape; documented rather than special-cased.
        ("A & & B", "A and and B"),
        # Known divergence from all three LML variants (see the Rust module
        # docs): this binding's whitespace check is Unicode `White_Space`
        # only, whereas Python's `\s` additionally matches the ASCII
        # info-separator control characters U+001C..=U+001F. Every LML
        # variant folds this; this implementation does not. Control
        # characters never appear in real names.
        ("A\x1c&\x1cB", "A\x1c&\x1cB"),
        # No ampersand: passthrough, including trailing whitespace (this
        # function does not trim).
        ("Stereolab", "Stereolab"),
        ("Stereolab  ", "Stereolab  "),
        # Diacritic-bearing WXYC-canonical names: proves the binding
        # round-trips multi-byte UTF-8 around the ASCII "&" untouched.
        ("Nilüfer Yanya", "Nilüfer Yanya"),
        ("Hermanos Gutiérrez", "Hermanos Gutiérrez"),
        ("Csillagrablók", "Csillagrablók"),
        (
            "Csillagrablók & Hermanos Gutiérrez",
            "Csillagrablók and Hermanos Gutiérrez",
        ),
        ("", ""),
    ],
)
def test_fold_conjunctions(input_str: str, expected: str) -> None:
    assert text.fold_conjunctions(input_str) == expected


def test_fold_conjunctions_idempotent() -> None:
    for s in ["Duke Ellington & John Coltrane", "R&B", "A & B & C", "Nilüfer Yanya", ""]:
        once = text.fold_conjunctions(s)
        twice = text.fold_conjunctions(once)
        assert once == twice, f"idempotence broken for {s!r}"


# --- None handling (wxyc-etl convention: Optional[str] -> "") ---


def test_fold_conjunctions_accepts_none() -> None:
    assert text.fold_conjunctions(None) == ""


def test_fold_conjunctions_still_accepts_str() -> None:
    assert text.fold_conjunctions("Stereolab") == "Stereolab"
