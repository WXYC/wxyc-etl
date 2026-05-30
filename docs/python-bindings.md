# Python Bindings (`wxyc-etl-python/`)

Mixed Rust/Python package laid out as:

```
wxyc-etl-python/
  src/                  # Rust extension, built as wxyc_etl._native
  python/wxyc_etl/      # pure-Python helpers
    __init__.py         # re-exports the Rust submodules
    logger.py           # Sentry + JSON logging (pure Python)
```

`pyproject.toml` has `python-source = "python"` and `module-name = "wxyc_etl._native"` so maturin packs both halves into a single `wxyc_etl` package. The Rust crate registers its submodules under `wxyc_etl.<name>` in `sys.modules` so consumers can use either `from wxyc_etl.text import ...` or `from wxyc_etl import text`. Pure-Python additions go in `python/wxyc_etl/*.py`.

Currently exposed Rust submodules: `text`, `parser`, `state`, `import_utils`, `schema`, `fuzzy`. The `pg`, `pipeline`, `csv_writer`, and `sqlite` modules are not exposed — Python consumers don't need them (they have psycopg/asyncpg directly).

Pure-Python helpers: `logger`.

## Install for downstream Python repos

From the consuming repo (e.g. discogs-etl, semantic-index):

```bash
pip install -e ../wxyc-etl/wxyc-etl-python
```

The `-e` editable install rebuilds the wheel via maturin on every `pip install`, so changes to the Rust source are picked up the next time you reinstall.

## Pure-Python fallback

Downstream consumers that have a Python implementation of the same logic (e.g. discogs-etl's `scripts/import_csv.py` and `scripts/verify_cache.py`) can force the slower Python path by setting:

```bash
WXYC_ETL_NO_RUST=1
```

Useful when debugging a Rust-vs-Python parity issue or when the wheel isn't built. The `_HAS_WXYC_ETL and not os.environ.get("WXYC_ETL_NO_RUST")` guard pattern is the convention.
