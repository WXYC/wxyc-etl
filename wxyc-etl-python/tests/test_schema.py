"""Tests for wxyc_etl.schema bindings."""

from wxyc_etl import schema


def test_discogs_tables():
    tables = schema.discogs_tables()
    assert "release" in tables
    assert "release_artist" in tables
    assert "artist" in tables
    assert len(tables) == 13


def test_discogs_release_columns():
    cols = schema.discogs_release_columns()
    assert "id" in cols
    assert "title" in cols
    assert "master_id" in cols


def test_library_ddl():
    ddl = schema.library_ddl()
    assert "CREATE TABLE" in ddl
    assert "artist" in ddl
    assert "id INTEGER PRIMARY KEY" in ddl


def test_library_columns():
    cols = schema.library_columns()
    assert "id" in cols
    assert "artist" in cols
    assert "title" in cols


def test_entity_identity_ddl():
    ddl = schema.entity_identity_ddl()
    assert "CREATE TABLE" in ddl
    assert "library_name" in ddl
    assert "discogs_artist_id" in ddl


def test_entity_identity_columns():
    cols = schema.entity_identity_columns()
    assert "library_name" in cols
    assert "wikidata_qid" in cols


def test_module_level_constants():
    assert schema.RELEASE_TABLE == "release"
    assert schema.ARTIST_TABLE == "artist"
    assert schema.LIBRARY_TABLE == "library"
    assert schema.ENTITY_IDENTITY_TABLE == "entity.identity"
    assert schema.CACHE_METADATA_TABLE == "cache_metadata"
