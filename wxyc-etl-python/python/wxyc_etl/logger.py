"""Sentry + structured JSON logging for WXYC ETL pipelines (Python side).

Mirrors :mod:`wxyc_etl::logger` (Rust). Every Python ETL should call
:func:`init_logger` once at startup with a ``repo`` and ``tool`` identifying
the pipeline. Logs emit one JSON object per line on the root logger with the
tags ``repo``, ``tool``, ``step``, and ``run_id`` (``step`` is set per-call via
:func:`logging.Logger.info` ``extra=`` or via :class:`LoggerStep`). When
``SENTRY_DSN`` is set in the environment (or supplied to :func:`init_logger`)
unhandled exceptions and ``logger.error`` events are forwarded to Sentry with
the same tags.

Usage::

    from wxyc_etl.logger import init_logger

    guard = init_logger(repo="discogs-etl", tool="discogs-etl daily-sync")

    import logging
    log = logging.getLogger(__name__)
    log.info("loaded recordings", extra={"step": "import", "rows": 42})

The returned :class:`LoggerGuard` flushes pending Sentry events when its
``flush`` method is called (or when the program exits).
"""

from __future__ import annotations

import atexit
import logging
import os
import sys
import uuid
from dataclasses import dataclass
from typing import Optional


@dataclass(frozen=True)
class LoggerConfig:
    """Configuration for :func:`init_logger`. ``sentry_dsn=None`` falls back to
    the ``SENTRY_DSN`` env var; ``run_id=None`` generates a UUIDv4."""

    repo: str
    tool: str
    sentry_dsn: Optional[str] = None
    run_id: Optional[str] = None


class LoggerGuard:
    """Returned by :func:`init_logger`. Call :meth:`flush` (or rely on the
    ``atexit`` hook) to drain Sentry before the process exits.
    """

    def __init__(self, run_id: str, sentry_enabled: bool) -> None:
        self.run_id = run_id
        self.sentry_enabled = sentry_enabled

    def flush(self, timeout: float = 2.0) -> None:
        if not self.sentry_enabled:
            return
        try:
            import sentry_sdk

            client = sentry_sdk.Hub.current.client
            if client is not None:
                client.flush(timeout=timeout)
        except Exception:  # pragma: no cover - flush is best-effort
            pass


class _ContextFilter(logging.Filter):
    """Inject repo/tool/run_id onto every record so the JSON formatter emits
    them. Per-event ``step`` is supplied by callers via ``extra={"step": ...}``
    and survives this filter unchanged.
    """

    def __init__(self, repo: str, tool: str, run_id: str) -> None:
        super().__init__()
        self._repo = repo
        self._tool = tool
        self._run_id = run_id

    def filter(self, record: logging.LogRecord) -> bool:
        if not hasattr(record, "repo"):
            record.repo = self._repo
        if not hasattr(record, "tool"):
            record.tool = self._tool
        if not hasattr(record, "run_id"):
            record.run_id = self._run_id
        if not hasattr(record, "step"):
            record.step = ""
        return True


_INITIALIZED = False


def init_logger(
    repo: str,
    tool: str,
    sentry_dsn: Optional[str] = None,
    run_id: Optional[str] = None,
    level: int = logging.INFO,
) -> LoggerGuard:
    """Initialize Sentry (when DSN is available) and JSON logging on the root
    logger. Idempotent: subsequent calls update tags but don't double-install
    handlers.
    """

    global _INITIALIZED

    rid = run_id or str(uuid.uuid4())
    dsn = sentry_dsn if sentry_dsn is not None else os.environ.get("SENTRY_DSN")
    sentry_enabled = bool(dsn)

    if sentry_enabled:
        import sentry_sdk

        sentry_sdk.init(
            dsn=dsn,
            traces_sample_rate=0.0,
            attach_stacktrace=True,
        )
        sentry_sdk.set_tag("repo", repo)
        sentry_sdk.set_tag("tool", tool)
        sentry_sdk.set_tag("run_id", rid)

    if not _INITIALIZED:
        try:
            from pythonjsonlogger.json import JsonFormatter
        except ImportError:
            try:
                from pythonjsonlogger.jsonlogger import (  # type: ignore[no-redef]
                    JsonFormatter,
                )
            except ImportError as exc:  # pragma: no cover - dep is declared
                raise RuntimeError(
                    "wxyc_etl.logger requires python-json-logger; "
                    "ensure the wxyc-etl wheel's runtime deps are installed"
                ) from exc

        formatter = JsonFormatter(
            "%(asctime)s %(levelname)s %(name)s %(message)s "
            "%(repo)s %(tool)s %(step)s %(run_id)s",
            rename_fields={"asctime": "timestamp", "levelname": "level"},
        )

        handler = logging.StreamHandler(sys.stderr)
        handler.setFormatter(formatter)
        handler.addFilter(_ContextFilter(repo, tool, rid))

        root = logging.getLogger()
        root.addHandler(handler)
        root.setLevel(level)

        guard = LoggerGuard(rid, sentry_enabled)
        atexit.register(guard.flush)
        _INITIALIZED = True
        return guard

    # Already initialized: reset the context filter on existing handlers so
    # the new repo/tool/run_id apply going forward.
    new_filter = _ContextFilter(repo, tool, rid)
    for handler in logging.getLogger().handlers:
        # Drop any prior _ContextFilter to avoid duplicate-tag injection.
        for existing in list(handler.filters):
            if isinstance(existing, _ContextFilter):
                handler.removeFilter(existing)
        handler.addFilter(new_filter)

    return LoggerGuard(rid, sentry_enabled)


__all__ = ["LoggerConfig", "LoggerGuard", "init_logger"]
