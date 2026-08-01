"""PyO3 parity for the Discogs disambiguation-suffix strip (wxyc-etl#147).

Locks the Python binding against the Rust implementation's decision table
(`wxyc-etl/src/text/disambiguation.rs`), which reconciles three overlapping
`library-metadata-lookup` suffix-stripping implementations. See that
module's doc comment for the full rationale behind each row below.
"""

from __future__ import annotations

import pytest
from wxyc_etl import text


@pytest.mark.parametrize(
    "input_str, broad, expected",
    [
        # Numeric suffix: all three LML variants agree — strip in both modes.
        ("Plug (2)", False, "Plug"),
        ("Plug (2)", True, "Plug"),
        ("Sessa (22)", False, "Sessa"),
        # Trailing whitespace after the suffix: DECISION — take the
        # broader, trailing-whitespace-tolerant LML pattern for
        # `broad=False` too (see the Rust module docs).
        ("Plug (2)   ", False, "Plug"),
        # Country/region code: only the broad LML variant strips this;
        # `broad` is an explicit caller-chosen parameter, not inferred.
        ("Stereolab (UK)", False, "Stereolab (UK)"),
        ("Stereolab (UK)", True, "Stereolab"),
        # Word-form qualifier: same narrow/broad split.
        ("Sade (band)", False, "Sade (band)"),
        ("Sade (band)", True, "Sade"),
        ("Pretty Girls (rapper)", True, "Pretty Girls"),
        # No suffix at all: no-op in both modes.
        ("Stereolab", False, "Stereolab"),
        ("Stereolab", True, "Stereolab"),
        # Empty parenthetical never matches.
        ("Foo ()", False, "Foo ()"),
        ("Foo ()", True, "Foo ()"),
        # Degenerate case: the entire input IS the suffix. Matches every
        # LML variant's `re.sub("", ...)` fidelity (no stem-preserving
        # guard, unlike `to_identity_match_form`'s trailing-paren strip).
        ("(2)", False, ""),
        ("(UK)", True, ""),
        # Empty input: no-op.
        ("", False, ""),
        ("", True, ""),
        # Diacritic-bearing WXYC-canonical names: proves the binding
        # preserves multi-byte UTF-8 on the stem while stripping the
        # (ASCII-only) suffix, or correctly leaving it alone under narrow
        # mode.
        ("Csillagrablók (2)", False, "Csillagrablók"),
        ("Hermanos Gutiérrez (Chile)", True, "Hermanos Gutiérrez"),
        ("Nilüfer Yanya (UK)", False, "Nilüfer Yanya (UK)"),
    ],
)
def test_strip_discogs_disambiguation(input_str: str, broad: bool, expected: str) -> None:
    assert text.strip_discogs_disambiguation(input_str, broad) == expected


def test_broad_content_length_cap() -> None:
    # Content = "nineteen characters" (19 chars) — exactly the bound. Strips.
    boundary_ok = "Foo (nineteen characters)"
    assert text.strip_discogs_disambiguation(boundary_ok, True) == "Foo"

    # Content = "nineteen charactersx" (20 chars) — one over. Left alone.
    boundary_over = "Foo (nineteen charactersx)"
    assert text.strip_discogs_disambiguation(boundary_over, True) == boundary_over


def test_strip_discogs_disambiguation_idempotent() -> None:
    for input_str, broad in [
        ("Plug (2)", False),
        ("Stereolab (UK)", True),
        ("Stereolab", False),
        ("", False),
    ]:
        once = text.strip_discogs_disambiguation(input_str, broad)
        twice = text.strip_discogs_disambiguation(once, broad)
        assert once == twice, f"idempotence broken for {input_str!r} broad={broad}"


# --- None handling (wxyc-etl convention: Optional[str] -> "") ---


def test_strip_discogs_disambiguation_accepts_none() -> None:
    assert text.strip_discogs_disambiguation(None, False) == ""
    assert text.strip_discogs_disambiguation(None, True) == ""


def test_strip_discogs_disambiguation_still_accepts_str() -> None:
    assert text.strip_discogs_disambiguation("Plug (2)", False) == "Plug"
