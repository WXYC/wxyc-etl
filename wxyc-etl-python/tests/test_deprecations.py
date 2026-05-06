"""WX-2.2.5: legacy normalizer entry points must fire DeprecationWarning.

The Python wrappers around `normalize_artist_name`, `strip_diacritics`, and
`batch_normalize` were superseded by the WX-2 charter forms (`to_storage_form`,
`to_match_form`, `to_ascii_form`). They still work, but consumers should
migrate. These tests assert the migration signal is firing.
"""

from __future__ import annotations

import warnings

import pytest
from wxyc_etl import text


@pytest.mark.parametrize(
    "call",
    [
        lambda: text.normalize_artist_name("Stereolab"),
        lambda: text.strip_diacritics("Nilüfer Yanya"),
        lambda: text.batch_normalize(["Stereolab"]),
    ],
    ids=["normalize_artist_name", "strip_diacritics", "batch_normalize"],
)
def test_legacy_entry_points_emit_deprecation_warning(call) -> None:
    with warnings.catch_warnings(record=True) as caught:
        warnings.simplefilter("always")
        call()
    assert any(issubclass(w.category, DeprecationWarning) for w in caught), (
        f"expected DeprecationWarning, got {[w.category.__name__ for w in caught]}"
    )


def test_charter_forms_do_not_emit_deprecation_warning() -> None:
    """The new WX-2 charter forms must NOT fire DeprecationWarning."""
    with warnings.catch_warnings(record=True) as caught:
        warnings.simplefilter("always")
        text.to_storage_form("Stereolab")
        text.to_match_form("Stereolab")
        text.to_ascii_form("Stereolab")
        text.batch_to_storage_form(["Stereolab"])
        text.batch_to_match_form(["Stereolab"])
        text.batch_to_ascii_form(["Stereolab"])
    deprecations = [w for w in caught if issubclass(w.category, DeprecationWarning)]
    assert deprecations == [], f"unexpected deprecation warnings: {deprecations}"
