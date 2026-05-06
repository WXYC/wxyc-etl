"""WX-2.2.4-followup: charter-form PyO3 wrappers must accept None.

The legacy `wxyc_etl.text.normalize_artist_name(None) -> ""` contract is a
load-bearing ergonomic — Python consumers commonly call the normalizer on a
DB column that may be NULL. The first cut of the WX-2 charter (0.2.0) made
`to_storage_form` / `to_match_form` / `to_ascii_form` raise `TypeError` on
None, forcing every caller to add `name or ""` guards. This test pins the
0.2.1 fix: same Optional[str] -> "" contract as the legacy.
"""

from __future__ import annotations

import pytest
from wxyc_etl import text


@pytest.mark.parametrize(
    "fn",
    [text.to_storage_form, text.to_match_form, text.to_ascii_form],
    ids=["to_storage_form", "to_match_form", "to_ascii_form"],
)
def test_charter_forms_accept_none(fn) -> None:
    assert fn(None) == ""


@pytest.mark.parametrize(
    "fn",
    [text.to_storage_form, text.to_match_form, text.to_ascii_form],
    ids=["to_storage_form", "to_match_form", "to_ascii_form"],
)
def test_charter_forms_still_accept_str(fn) -> None:
    """Sanity: the None-overload didn't break the str path."""
    assert fn("Stereolab") != ""
