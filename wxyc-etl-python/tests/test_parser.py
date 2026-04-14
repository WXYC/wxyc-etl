"""Tests for wxyc_etl.parser bindings (backward compatible with sql_parser_rs)."""

import os
import tempfile

from wxyc_etl import parser


def test_load_table_rows_basic():
    with tempfile.NamedTemporaryFile(mode="w", suffix=".sql", delete=False) as f:
        f.write(
            "INSERT INTO `GENRE` VALUES (1,'Rock'),(2,'Jazz'),(3,'Electronic');\n"
        )
        f.flush()
        path = f.name

    try:
        rows = parser.load_table_rows(path, "GENRE")
        assert len(rows) == 3
        assert rows[0] == (1, "Rock")
        assert rows[1] == (2, "Jazz")
        assert rows[2] == (3, "Electronic")
    finally:
        os.unlink(path)


def test_load_table_rows_null_handling():
    with tempfile.NamedTemporaryFile(mode="w", suffix=".sql", delete=False) as f:
        f.write("INSERT INTO `data` VALUES (1,NULL,'text');\n")
        f.flush()
        path = f.name

    try:
        rows = parser.load_table_rows(path, "data")
        assert len(rows) == 1
        assert rows[0] == (1, None, "text")
    finally:
        os.unlink(path)


def test_load_table_rows_float():
    with tempfile.NamedTemporaryFile(mode="w", suffix=".sql", delete=False) as f:
        f.write("INSERT INTO `data` VALUES (1,3.14,'pi');\n")
        f.flush()
        path = f.name

    try:
        rows = parser.load_table_rows(path, "data")
        assert len(rows) == 1
        assert rows[0][0] == 1
        assert abs(rows[0][1] - 3.14) < 0.001
        assert rows[0][2] == "pi"
    finally:
        os.unlink(path)


def test_load_table_rows_escaped_quote():
    with tempfile.NamedTemporaryFile(mode="w", suffix=".sql", delete=False) as f:
        f.write("INSERT INTO `data` VALUES (1,'HONEST JON\\'S/ASTRALWERKS');\n")
        f.flush()
        path = f.name

    try:
        rows = parser.load_table_rows(path, "data")
        assert len(rows) == 1
        assert rows[0][1] == "HONEST JON'S/ASTRALWERKS"
    finally:
        os.unlink(path)


def test_load_table_rows_ignores_other_tables():
    with tempfile.NamedTemporaryFile(mode="w", suffix=".sql", delete=False) as f:
        f.write(
            "INSERT INTO `wanted` VALUES (1,'yes');\n"
            "INSERT INTO `unwanted` VALUES (2,'no');\n"
        )
        f.flush()
        path = f.name

    try:
        rows = parser.load_table_rows(path, "wanted")
        assert len(rows) == 1
        assert rows[0] == (1, "yes")
    finally:
        os.unlink(path)


def test_load_table_rows_empty_file():
    with tempfile.NamedTemporaryFile(mode="w", suffix=".sql", delete=False) as f:
        f.write("")
        f.flush()
        path = f.name

    try:
        rows = parser.load_table_rows(path, "data")
        assert rows == []
    finally:
        os.unlink(path)


def test_iter_table_rows_compatibility():
    """iter_table_rows should return the same result as load_table_rows."""
    with tempfile.NamedTemporaryFile(mode="w", suffix=".sql", delete=False) as f:
        f.write("INSERT INTO `data` VALUES (1,'a'),(2,'b');\n")
        f.flush()
        path = f.name

    try:
        loaded = parser.load_table_rows(path, "data")
        iterated = parser.iter_table_rows(path, "data")
        assert loaded == iterated
    finally:
        os.unlink(path)


def test_parse_sql_values_basic():
    rows = parser.parse_sql_values("(1,'hello',NULL)")
    assert len(rows) == 1
    assert rows[0][0] == 1
    assert rows[0][1] == "hello"
    assert rows[0][2] is None


def test_parse_sql_values_multiple_rows():
    rows = parser.parse_sql_values("(1,'a'),(2,'b'),(3,'c')")
    assert len(rows) == 3
    assert rows[0][0] == 1
    assert rows[2][0] == 3


def test_parse_sql_values_empty():
    rows = parser.parse_sql_values("")
    assert rows == []
