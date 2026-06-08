"""Python bindings for the wxyc-etl Rust crate, plus pure-Python helpers.

The Rust extension lives at :mod:`wxyc_etl._native`; its submodules
(``text``, ``parser``, ``pg``, ``state``, ``import_utils``, ``schema``, ``fuzzy``) are
re-exported here so callers can use ``from wxyc_etl import text`` and
``from wxyc_etl.text import to_match_form`` interchangeably.

The :mod:`wxyc_etl.logger` module is pure Python; see its docstring for usage.
"""

from . import _native
from ._native import (
    fuzzy,
    import_utils,
    parser,
    pg,
    schema,
    state,
    text,
)

__all__ = [
    "fuzzy",
    "import_utils",
    "logger",
    "parser",
    "pg",
    "schema",
    "state",
    "text",
]
