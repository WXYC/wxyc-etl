"""PyO3 parity for the cross-cache-identity layered normalizer.

Locks the Python binding against the Rust implementation's behavior. Cases
mirror `wxyc-etl/src/text/identity.rs`'s unit tests; the full parity matrix
ships in a follow-up PR (epic #99).

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
