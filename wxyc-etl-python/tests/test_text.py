"""Tests for wxyc_etl.text bindings."""

import pytest
from wxyc_etl import text


# --- normalize_artist_name ---


@pytest.mark.parametrize(
    "input_name,expected",
    [
        ("Stereolab", "stereolab"),
        ("  Stereolab  ", "stereolab"),
        ("STEREOLAB", "stereolab"),
        ("  Mixed Case  ", "mixed case"),
        ("", ""),
        ("Nilüfer Yanya", "nilufer yanya"),
        ("Csillagrablók", "csillagrablok"),
        ("Hermanos Gutiérrez", "hermanos gutierrez"),
        # ñ (combining-tilde decomposition) — canonical from @wxyc/shared#81
        ("Sonido Dueñez", "sonido duenez"),
        # Multi-diacritic Turkish — canonical from @wxyc/shared#81. Note:
        # ı (dotless i) does not decompose under NFKD; only ş is stripped.
        ("Aşıq Altay", "asıq altay"),
    ],
    ids=[
        "lowercase",
        "strip_spaces",
        "all_caps",
        "mixed_case_strip",
        "empty",
        "nilufer_yanya",
        "csillagrablok",
        "hermanos_gutierrez",
        "sonido_duenez",
        "asiq_altay",
    ],
)
def test_normalize_artist_name(input_name, expected):
    assert text.normalize_artist_name(input_name) == expected


def test_normalize_artist_name_none():
    """None input should return empty string, not raise TypeError."""
    assert text.normalize_artist_name(None) == ""


# --- strip_diacritics ---


@pytest.mark.parametrize(
    "input_str,expected",
    [
        ("Nilüfer Yanya", "Nilufer Yanya"),
        ("  Hermanos Gutiérrez  ", "  Hermanos Gutierrez  "),
        ("Stereolab", "Stereolab"),
        ("", ""),
    ],
)
def test_strip_diacritics(input_str, expected):
    assert text.strip_diacritics(input_str) == expected


# --- batch_normalize ---


def test_batch_normalize():
    results = text.batch_normalize(["Nilüfer Yanya", "Stereolab", "Csillagrablók"])
    assert results == ["nilufer yanya", "stereolab", "csillagrablok"]


def test_batch_normalize_empty():
    assert text.batch_normalize([]) == []


# --- is_compilation_artist ---


@pytest.mark.parametrize(
    "name,expected",
    [
        ("Various Artists", True),
        ("Soundtrack", True),
        ("V/A", True),
        ("v.a.", True),
        ("Autechre", False),
        ("Stereolab", False),
        ("", False),
    ],
)
def test_is_compilation_artist(name, expected):
    assert text.is_compilation_artist(name) is expected


# --- split_artist_name ---


def test_split_artist_name_comma():
    result = text.split_artist_name("Yo La Tengo, Stereolab, Autechre")
    assert result == ["Yo La Tengo", "Stereolab", "Autechre"]


def test_split_artist_name_slash():
    result = text.split_artist_name("J Dilla / Jay Dee")
    assert result == ["J Dilla", "Jay Dee"]


def test_split_artist_name_no_split():
    assert text.split_artist_name("Autechre") is None


def test_split_artist_name_ampersand_no_context():
    """Ampersand should NOT split without context."""
    assert text.split_artist_name("Duke Ellington & John Coltrane") is None


def test_split_artist_name_numeric_guard():
    # Non-canonical name kept as a guard test input: leading "<digits>," should
    # not split on the comma.
    assert text.split_artist_name("10,000 Maniacs") is None


# --- split_artist_name_contextual ---


def test_split_artist_name_contextual_with_known():
    known = {"duke ellington", "john coltrane"}
    result = text.split_artist_name_contextual(
        "Duke Ellington & John Coltrane", known
    )
    assert result is not None
    assert "Duke Ellington" in result
    assert "John Coltrane" in result


def test_split_artist_name_contextual_no_known():
    result = text.split_artist_name_contextual("Yo La Tengo & Animal Collective", set())
    assert result is None


def test_split_artist_name_contextual_falls_back_to_context_free():
    result = text.split_artist_name_contextual("J Dilla / Jay Dee", set())
    assert result == ["J Dilla", "Jay Dee"]
