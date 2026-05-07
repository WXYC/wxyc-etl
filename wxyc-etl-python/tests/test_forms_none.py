"""WX-2.2.4-followup: charter-form PyO3 wrappers must accept None.

The legacy `wxyc_etl.text.normalize_artist_name(None) -> ""` contract is a
load-bearing ergonomic — Python consumers commonly call the normalizer on a
DB column that may be NULL. The first cut of the WX-2 charter (0.2.0) made
`to_storage_form` / `to_match_form` / `to_ascii_form` raise `TypeError` on
None, forcing every caller to add `name or ""` guards. This test pins the
0.2.1 fix: same Optional[str] -> "" contract as the legacy.

Extended in E3 step 4 to cover the four cross-cache-identity entry points.
"""

from __future__ import annotations

import pytest
from wxyc_etl import text


_FORMS = [
    text.to_storage_form,
    text.to_match_form,
    text.to_ascii_form,
    text.to_identity_match_form,
    text.to_identity_match_form_title,
    text.to_identity_match_form_with_punctuation,
    text.to_identity_match_form_with_disambiguator_strip,
]
_IDS = [
    "to_storage_form",
    "to_match_form",
    "to_ascii_form",
    "to_identity_match_form",
    "to_identity_match_form_title",
    "to_identity_match_form_with_punctuation",
    "to_identity_match_form_with_disambiguator_strip",
]


@pytest.mark.parametrize("fn", _FORMS, ids=_IDS)
def test_charter_forms_accept_none(fn) -> None:
    assert fn(None) == ""


@pytest.mark.parametrize("fn", _FORMS, ids=_IDS)
def test_charter_forms_still_accept_str(fn) -> None:
    """Sanity: the None-overload didn't break the str path."""
    assert fn("Stereolab") != ""
