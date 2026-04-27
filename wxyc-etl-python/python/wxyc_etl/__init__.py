"""Python bindings for the wxyc-etl Rust crate, plus pure-Python helpers.

The Rust extension lives at :mod:`wxyc_etl._native`; its submodules
(``text``, ``parser``, ``state``, ``import_utils``, ``schema``, ``fuzzy``) are
re-exported here so callers can use the historical ``from wxyc_etl import
text`` and ``from wxyc_etl.text import normalize_artist_name`` forms
interchangeably.

The :mod:`wxyc_etl.logger` module is pure Python; see its docstring for usage.
"""

from . import _native
from ._native import (
    fuzzy,
    import_utils,
    parser,
    schema,
    state,
    text,
)

__all__ = [
    "fuzzy",
    "import_utils",
    "logger",
    "parser",
    "schema",
    "state",
    "text",
]
