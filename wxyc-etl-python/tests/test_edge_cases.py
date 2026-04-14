"""PyO3 type conversion edge case tests.

Validates that every PyO3-exposed function handles boundary inputs without
panicking: None, empty strings, whitespace, non-BMP Unicode, combining
characters, null bytes, very long strings, and mixed valid/invalid batches.
"""

import os
import tempfile

import pytest

from wxyc_etl import text, parser, fuzzy
from wxyc_etl.state import PipelineState
from wxyc_etl.import_utils import DedupSet


# ---------------------------------------------------------------------------
# Shared edge-case inputs
# ---------------------------------------------------------------------------

# Inputs safe for O(n) functions (text normalization, DedupSet, PipelineState).
EDGE_STRINGS = [
    pytest.param("", id="empty"),
    pytest.param("   ", id="whitespace-only"),
    pytest.param("\U0001f3b5", id="emoji-\U0001f3b5"),
    pytest.param("\u97f3\u697d", id="cjk-\u97f3\u697d"),
    pytest.param("O\u0308", id="combining-umlaut"),
    pytest.param("\x00", id="null-byte"),
    pytest.param("a" * 1_000_000, id="1MB-string"),
]

EDGE_STRINGS_WITH_NONE = [
    pytest.param(None, id="none"),
    *EDGE_STRINGS,
]

# Inputs safe for O(n^2) fuzzy metrics (Jaro-Winkler, token_set, token_sort).
# Excludes the 1MB string which would take minutes in quadratic algorithms,
# but includes a 10K string to still exercise long-input handling.
FUZZY_EDGE_STRINGS = [
    pytest.param("", id="empty"),
    pytest.param("   ", id="whitespace-only"),
    pytest.param("\U0001f3b5", id="emoji-\U0001f3b5"),
    pytest.param("\u97f3\u697d", id="cjk-\u97f3\u697d"),
    pytest.param("O\u0308", id="combining-umlaut"),
    pytest.param("\x00", id="null-byte"),
    pytest.param("a" * 10_000, id="10K-string"),
]


# ===================================================================
# text module
# ===================================================================


class TestNormalizeArtistNameEdgeCases:
    """normalize_artist_name accepts Option<&str>; None should return ""."""

    @pytest.mark.parametrize("input_val", EDGE_STRINGS_WITH_NONE)
    def test_no_panic_returns_string(self, input_val):
        result = text.normalize_artist_name(input_val)
        assert isinstance(result, str)

    def test_none_returns_empty(self):
        assert text.normalize_artist_name(None) == ""

    def test_empty_returns_empty(self):
        assert text.normalize_artist_name("") == ""

    def test_whitespace_returns_empty(self):
        assert text.normalize_artist_name("   ") == ""

    def test_combining_character_stripped(self):
        # O + combining diaeresis should normalize equivalently to "o"
        result = text.normalize_artist_name("O\u0308")
        assert isinstance(result, str)
        assert len(result) <= 2  # should be "o" after NFKD + strip diacritics

    def test_1mb_string_no_panic(self):
        result = text.normalize_artist_name("a" * 1_000_000)
        assert isinstance(result, str)


class TestStripDiacriticsEdgeCases:
    @pytest.mark.parametrize("input_val", EDGE_STRINGS)
    def test_no_panic_returns_string(self, input_val):
        result = text.strip_diacritics(input_val)
        assert isinstance(result, str)

    def test_empty_returns_empty(self):
        assert text.strip_diacritics("") == ""

    def test_combining_character(self):
        result = text.strip_diacritics("O\u0308")
        assert isinstance(result, str)


class TestBatchNormalizeEdgeCases:
    def test_empty_list(self):
        assert text.batch_normalize([]) == []

    def test_mixed_valid_invalid(self):
        """One bad input should not corrupt the valid ones."""
        inputs = ["Stereolab", "", "   ", "\x00", "Cat Power"]
        results = text.batch_normalize(inputs)
        assert len(results) == 5
        assert results[0] == "stereolab"
        assert results[4] == "cat power"
        assert all(isinstance(r, str) for r in results)

    def test_all_edge_strings(self):
        inputs = ["", "   ", "\U0001f3b5", "\u97f3\u697d", "O\u0308", "\x00"]
        results = text.batch_normalize(inputs)
        assert len(results) == len(inputs)
        assert all(isinstance(r, str) for r in results)


class TestIsCompilationArtistEdgeCases:
    @pytest.mark.parametrize("input_val", EDGE_STRINGS)
    def test_no_panic_returns_bool(self, input_val):
        result = text.is_compilation_artist(input_val)
        assert isinstance(result, bool)

    def test_empty_returns_false(self):
        assert text.is_compilation_artist("") is False

    def test_whitespace_returns_false(self):
        assert text.is_compilation_artist("   ") is False


class TestSplitArtistNameEdgeCases:
    @pytest.mark.parametrize("input_val", EDGE_STRINGS)
    def test_no_panic(self, input_val):
        result = text.split_artist_name(input_val)
        assert result is None or isinstance(result, list)

    def test_empty_returns_none(self):
        assert text.split_artist_name("") is None

    def test_whitespace_returns_none(self):
        assert text.split_artist_name("   ") is None


class TestSplitArtistNameContextualEdgeCases:
    @pytest.mark.parametrize("input_val", EDGE_STRINGS)
    def test_no_panic(self, input_val):
        result = text.split_artist_name_contextual(input_val, set())
        assert result is None or isinstance(result, list)

    def test_empty_with_known(self):
        result = text.split_artist_name_contextual("", {"stereolab"})
        assert result is None or isinstance(result, list)

    def test_edge_case_in_known_set(self):
        """Edge-case strings in the known_artists set shouldn't cause panics."""
        known = {"", "\x00", "\U0001f3b5"}
        result = text.split_artist_name_contextual("Stereolab", known)
        assert result is None or isinstance(result, list)


# ===================================================================
# fuzzy module
# ===================================================================


class TestJaroWinklerSimilarityEdgeCases:
    @pytest.mark.parametrize("input_val", FUZZY_EDGE_STRINGS)
    def test_no_panic_self_comparison(self, input_val):
        result = fuzzy.jaro_winkler_similarity(input_val, input_val)
        assert isinstance(result, float)
        assert 0.0 <= result <= 1.0

    @pytest.mark.parametrize("input_val", FUZZY_EDGE_STRINGS)
    def test_no_panic_against_normal(self, input_val):
        result = fuzzy.jaro_winkler_similarity(input_val, "Stereolab")
        assert isinstance(result, float)
        assert 0.0 <= result <= 1.0

    def test_both_empty(self):
        assert fuzzy.jaro_winkler_similarity("", "") == 1.0


class TestTokenSetRatioEdgeCases:
    @pytest.mark.parametrize("input_val", FUZZY_EDGE_STRINGS)
    def test_no_panic_self_comparison(self, input_val):
        result = fuzzy.token_set_ratio(input_val, input_val)
        assert isinstance(result, float)
        assert 0.0 <= result <= 1.0

    @pytest.mark.parametrize("input_val", FUZZY_EDGE_STRINGS)
    def test_no_panic_against_normal(self, input_val):
        result = fuzzy.token_set_ratio(input_val, "Stereolab")
        assert isinstance(result, float)
        assert 0.0 <= result <= 1.0

    def test_both_empty(self):
        assert fuzzy.token_set_ratio("", "") == 1.0


class TestTokenSortRatioEdgeCases:
    @pytest.mark.parametrize("input_val", FUZZY_EDGE_STRINGS)
    def test_no_panic_self_comparison(self, input_val):
        result = fuzzy.token_sort_ratio(input_val, input_val)
        assert isinstance(result, float)
        assert 0.0 <= result <= 1.0

    @pytest.mark.parametrize("input_val", FUZZY_EDGE_STRINGS)
    def test_no_panic_against_normal(self, input_val):
        result = fuzzy.token_sort_ratio(input_val, "Stereolab")
        assert isinstance(result, float)
        assert 0.0 <= result <= 1.0

    def test_both_empty(self):
        assert fuzzy.token_sort_ratio("", "") == 1.0


class TestBatchFuzzyResolveEdgeCases:
    def test_empty_names(self):
        results = fuzzy.batch_fuzzy_resolve([], ["Autechre"], 0.8)
        assert results == []

    def test_empty_catalog(self):
        results = fuzzy.batch_fuzzy_resolve(["Autechre"], [], 0.8)
        assert results == [None]

    def test_both_empty(self):
        results = fuzzy.batch_fuzzy_resolve([], [], 0.8)
        assert results == []

    def test_edge_case_names(self):
        edge_names = ["", "   ", "\x00", "\U0001f3b5"]
        catalog = ["Autechre", "Stereolab"]
        results = fuzzy.batch_fuzzy_resolve(edge_names, catalog, 0.8)
        assert len(results) == len(edge_names)
        assert all(r is None or isinstance(r, str) for r in results)

    def test_edge_case_catalog(self):
        names = ["Autechre"]
        edge_catalog = ["", "\x00", "\U0001f3b5"]
        results = fuzzy.batch_fuzzy_resolve(names, edge_catalog, 0.0)
        assert len(results) == 1
        assert results[0] is None or isinstance(results[0], str)

    def test_mixed_valid_invalid_names(self):
        """Valid name should resolve correctly even when surrounded by bad inputs."""
        names = ["\x00", "Autechre", "   "]
        catalog = ["Autechre", "Stereolab"]
        results = fuzzy.batch_fuzzy_resolve(names, catalog, 0.8)
        assert len(results) == 3
        assert results[1] == "Autechre"


class TestBatchFilterArtistsEdgeCases:
    def test_empty_names(self):
        assert fuzzy.batch_filter_artists([], set()) == []

    def test_empty_library(self):
        results = fuzzy.batch_filter_artists(["Autechre"], set())
        assert results == [False]

    def test_edge_case_names(self):
        edge_names = ["", "   ", "\x00", "\U0001f3b5", "O\u0308"]
        library = {"autechre"}
        results = fuzzy.batch_filter_artists(edge_names, library)
        assert len(results) == len(edge_names)
        assert all(isinstance(r, bool) for r in results)

    def test_edge_case_in_library(self):
        """Edge-case strings in the library set shouldn't cause panics."""
        library = {"", "\x00", "autechre"}
        results = fuzzy.batch_filter_artists(["Autechre", "", "\x00"], library)
        assert len(results) == 3
        assert all(isinstance(r, bool) for r in results)

    def test_mixed_valid_invalid(self):
        """Valid match should not be corrupted by edge-case inputs."""
        library = {"autechre", "stereolab"}
        names = ["\x00", "Autechre", "   ", "Stereolab"]
        results = fuzzy.batch_filter_artists(names, library)
        assert len(results) == 4
        assert results[1] is True
        assert results[3] is True


class TestBatchClassifyReleasesEdgeCases:
    LIBRARY_PAIRS = [
        ("Juana Molina", "DOGA"),
        ("Stereolab", "Aluminum Tunes"),
    ]

    def test_empty(self):
        results = fuzzy.batch_classify_releases([], [], self.LIBRARY_PAIRS)
        assert results == []

    def test_mismatched_lengths_raises(self):
        with pytest.raises(ValueError):
            fuzzy.batch_classify_releases(["a"], ["b", "c"], self.LIBRARY_PAIRS)

    def test_edge_case_artists_and_titles(self):
        edge = ["", "   ", "\x00", "\U0001f3b5"]
        results = fuzzy.batch_classify_releases(edge, edge, self.LIBRARY_PAIRS)
        assert len(results) == len(edge)
        assert all(r in ("keep", "prune", "review") for r in results)

    def test_empty_library_pairs(self):
        results = fuzzy.batch_classify_releases(
            ["Autechre"], ["Confield"], []
        )
        assert len(results) == 1
        assert results[0] in ("keep", "prune", "review")

    def test_mixed_valid_invalid(self):
        """Valid pair should still classify correctly alongside bad inputs."""
        artists = ["\x00", "Juana Molina", "   "]
        titles = ["\x00", "DOGA", "   "]
        results = fuzzy.batch_classify_releases(
            artists, titles, self.LIBRARY_PAIRS
        )
        assert len(results) == 3
        assert results[1] == "keep"
        assert all(r in ("keep", "prune", "review") for r in results)


# ===================================================================
# parser module
# ===================================================================


class TestParseSqlValuesEdgeCases:
    @pytest.mark.parametrize(
        "input_val",
        [
            pytest.param("", id="empty"),
            pytest.param("   ", id="whitespace-only"),
        ],
    )
    def test_returns_empty_list(self, input_val):
        result = parser.parse_sql_values(input_val)
        assert result == []

    def test_unicode_in_values(self):
        result = parser.parse_sql_values("(1,'\U0001f3b5')")
        assert len(result) == 1
        assert result[0][1] == "\U0001f3b5"

    def test_cjk_in_values(self):
        result = parser.parse_sql_values("(1,'\u97f3\u697d')")
        assert len(result) == 1
        assert result[0][1] == "\u97f3\u697d"

    def test_null_byte_in_string_value(self):
        """Null byte embedded in a SQL string literal."""
        result = parser.parse_sql_values("(1,'ab\\0cd')")
        assert len(result) == 1
        assert isinstance(result[0][1], str)


# ===================================================================
# DedupSet (import_utils)
# ===================================================================


class TestDedupSetEdgeCases:
    def test_none_values(self):
        ds = DedupSet()
        assert ds.add([None, None]) is True
        assert ds.add(["", ""]) is False  # None == ""
        assert len(ds) == 1

    def test_unicode_keys(self):
        ds = DedupSet()
        assert ds.add(["\U0001f3b5", "\u97f3\u697d"]) is True
        assert ds.add(["\U0001f3b5", "\u97f3\u697d"]) is False
        assert len(ds) == 1

    def test_empty_key(self):
        ds = DedupSet()
        assert ds.add([]) is True
        assert ds.add([]) is False

    def test_null_byte_key(self):
        ds = DedupSet()
        assert ds.add(["\x00"]) is True
        assert ds.add(["\x00"]) is False
        assert len(ds) == 1

    def test_1mb_string_key(self):
        ds = DedupSet()
        big = "a" * 1_000_000
        assert ds.add([big]) is True
        assert ds.add([big]) is False
        assert len(ds) == 1

    def test_contains_with_edge_cases(self):
        ds = DedupSet()
        ds.add(["\U0001f3b5"])
        assert ["\U0001f3b5"] in ds
        assert ["\x00"] not in ds

    def test_mixed_none_and_strings(self):
        ds = DedupSet()
        ds.add([None, "title"])
        assert ["", "title"] in ds
        assert [None, "title"] in ds


# ===================================================================
# PipelineState (state)
# ===================================================================


class TestPipelineStateEdgeCases:
    def test_empty_steps(self):
        state = PipelineState("db_url", "csv_dir", [])
        assert state.step_status("anything") == "unknown"

    def test_unicode_step_names(self):
        state = PipelineState("db_url", "csv_dir", ["\U0001f3b5", "\u97f3\u697d"])
        assert state.step_status("\U0001f3b5") == "pending"
        state.mark_completed("\U0001f3b5")
        assert state.is_completed("\U0001f3b5")
        assert not state.is_completed("\u97f3\u697d")

    def test_empty_string_step(self):
        state = PipelineState("db_url", "csv_dir", [""])
        assert state.step_status("") == "pending"
        state.mark_completed("")
        assert state.is_completed("")

    def test_unicode_error_message(self):
        state = PipelineState("db_url", "csv_dir", ["step1"])
        state.mark_failed("step1", "\U0001f3b5 error \u97f3\u697d")
        assert state.step_error("step1") == "\U0001f3b5 error \u97f3\u697d"

    def test_save_load_with_unicode(self):
        with tempfile.TemporaryDirectory() as d:
            path = os.path.join(d, "state.json")
            state = PipelineState("db\U0001f3b5", "csv\u97f3\u697d", ["\U0001f3b5"])
            state.mark_completed("\U0001f3b5")
            state.save(path)

            loaded = PipelineState.load(path)
            assert loaded.is_completed("\U0001f3b5")

    def test_null_byte_step_name(self):
        state = PipelineState("db", "csv", ["\x00"])
        assert state.step_status("\x00") == "pending"
        state.mark_completed("\x00")
        assert state.is_completed("\x00")

    def test_1mb_error_message(self):
        state = PipelineState("db", "csv", ["step1"])
        big_msg = "x" * 1_000_000
        state.mark_failed("step1", big_msg)
        assert state.step_error("step1") == big_msg
