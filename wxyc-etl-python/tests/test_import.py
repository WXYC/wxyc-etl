"""Test that wxyc_etl module imports and exposes expected submodules."""


def test_import_wxyc_etl():
    import wxyc_etl

    assert hasattr(wxyc_etl, "text")
    assert hasattr(wxyc_etl, "parser")
    assert hasattr(wxyc_etl, "pg")
    assert hasattr(wxyc_etl, "state")
    assert hasattr(wxyc_etl, "import_utils")
    assert hasattr(wxyc_etl, "schema")


def test_text_submodule_functions():
    from wxyc_etl import text

    assert callable(text.to_storage_form)
    assert callable(text.to_match_form)
    assert callable(text.to_ascii_form)
    assert callable(text.batch_to_match_form)
    assert callable(text.is_compilation_artist)
    assert callable(text.split_artist_name)
    assert callable(text.split_artist_name_contextual)


def test_parser_submodule_functions():
    from wxyc_etl import parser

    assert callable(parser.load_table_rows)
    assert callable(parser.iter_table_rows)
    assert callable(parser.parse_sql_values)


def test_pg_submodule_functions():
    from wxyc_etl import pg

    assert callable(pg.to_pg_text_form)
    assert callable(pg.batch_to_pg_text_form)


def test_state_submodule_class():
    from wxyc_etl import state

    assert hasattr(state, "PipelineState")


def test_import_utils_submodule_class():
    from wxyc_etl import import_utils

    assert hasattr(import_utils, "DedupSet")


def test_schema_submodule():
    from wxyc_etl import schema

    assert callable(schema.discogs_tables)
    assert callable(schema.library_ddl)
    assert callable(schema.entity_identity_ddl)
