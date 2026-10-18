"""Sentry + structured JSON logging for WXYC ETL pipelines (Python side).

Mirrors :mod:`wxyc_etl::logger` (Rust). Every Python ETL should call
:func:`init_logger` once at startup with a ``repo`` and ``tool`` identifying
the pipeline. Logs emit one JSON object per line on the root logger with the
tags ``repo``, ``tool``, ``step``, and ``run_id`` (``step`` is set per-call via
:func:`logging.Logger.info` ``extra=`` or via :class:`LoggerStep`). When
``SENTRY_DSN`` is set in the environment (or supplied to :func:`init_logger`)
unhandled exceptions and ``logger.error`` events are forwarded to Sentry with
the same tags. When ``POSTHOG_ERRORS_API_KEY`` is set and the optional
``posthog`` extra is installed, ERROR records are also sent to PostHog error
tracking (see :class:`_PosthogErrorHandler`); Sentry keeps working alongside.

Usage::

    from wxyc_etl.logger import init_logger

    guard = init_logger(repo="discogs-etl", tool="discogs-etl daily-sync")

    import logging
    log = logging.getLogger(__name__)
    log.info("loaded recordings", extra={"step": "import", "rows": 42})

The returned :class:`LoggerGuard` flushes pending Sentry and PostHog events
when its ``flush`` method is called (or when the program exits).
"""

from __future__ import annotations

import atexit
import logging
import os
import sys
import threading
import time
import uuid
from dataclasses import dataclass
from typing import Any, Callable, Optional

# Backstop constants, kept in step with wxyc-fastapi's ERROR-to-PostHog handler
# (ERRORS_BURST = 100, ERRORS_EVENTS_PER_MINUTE = 30, 60s dedupe window).
POSTHOG_ERRORS_BUCKET_CAPACITY = 100
POSTHOG_ERRORS_REFILL_PER_SECOND = 30 / 60
POSTHOG_ERRORS_DEDUPE_WINDOW_SECONDS = 60.0
_MAX_TRACKED_FINGERPRINTS = 512
_DEFAULT_POSTHOG_HOST = "https://us.i.posthog.com"
# Loggers whose output must never be re-captured (the SDK logs through these).
_POSTHOG_SKIP_LOGGERS = ("posthog", "backoff", "wxyc_etl")


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
    ``atexit`` hook) to drain Sentry and the PostHog errors client before the
    process exits.
    """

    def __init__(
        self, run_id: str, sentry_enabled: bool, posthog_client: Any = None
    ) -> None:
        self.run_id = run_id
        self.sentry_enabled = sentry_enabled
        self._posthog_client = posthog_client

    def flush(self, timeout: float = 2.0) -> None:
        if self._posthog_client is not None:
            try:
                self._posthog_client.flush()
            except Exception:  # pragma: no cover - flush is best-effort
                pass
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


class _PosthogErrorHandler(logging.Handler):
    """Send ERROR records to PostHog error tracking.

    Records with ``exc_info`` go through ``capture_exception``; the rest become
    a synthesized ``$exception`` event fingerprinted by logger name and
    unformatted message template, joined with ``:`` into the single string
    PostHog expects for ``$exception_fingerprint``. A same-fingerprint dedupe
    window and a token bucket bound the volume so an error loop cannot flood
    the project.
    """

    def __init__(
        self,
        client: Any,
        clock: Callable[[], float] = time.monotonic,
        capacity: float = POSTHOG_ERRORS_BUCKET_CAPACITY,
        refill_per_second: float = POSTHOG_ERRORS_REFILL_PER_SECOND,
        dedupe_window: float = POSTHOG_ERRORS_DEDUPE_WINDOW_SECONDS,
    ) -> None:
        super().__init__(level=logging.ERROR)
        self._client = client
        self._clock = clock
        self._capacity = capacity
        self._refill = refill_per_second
        self._window = dedupe_window
        self._tokens = float(capacity)
        self._last_refill = clock()
        self._seen: dict[tuple, float] = {}
        self._guard = threading.local()

    def _admit(self, key: tuple) -> bool:
        now = self._clock()
        with self.lock:
            last = self._seen.get(key)
            if last is not None and now - last < self._window:
                return False
            elapsed = now - self._last_refill
            self._last_refill = now
            self._tokens = min(self._capacity, self._tokens + elapsed * self._refill)
            if self._tokens < 1:
                return False
            self._tokens -= 1
            if len(self._seen) >= _MAX_TRACKED_FINGERPRINTS:
                self._seen = {k: t for k, t in self._seen.items() if now - t < self._window}
                if len(self._seen) >= _MAX_TRACKED_FINGERPRINTS:
                    self._seen.clear()
            self._seen[key] = now
            return True

    def emit(self, record: logging.LogRecord) -> None:
        if record.name.split(".")[0] in _POSTHOG_SKIP_LOGGERS:
            return
        if getattr(self._guard, "active", False):
            return
        exc_type = record.exc_info[0] if record.exc_info else None
        fingerprint = f"{record.name}:{record.msg}"
        if not self._admit((fingerprint, exc_type)):
            return
        props = {
            key: getattr(record, key, "")
            for key in ("repo", "tool", "run_id", "step")
        }
        props["$process_person_profile"] = False
        distinct_id = props["run_id"] or record.name
        self._guard.active = True
        try:
            if record.exc_info and record.exc_info[0] is not None:
                self._client.capture_exception(
                    record.exc_info, distinct_id=distinct_id, properties=props
                )
            else:
                props["$exception_list"] = [
                    {
                        "type": record.name,
                        "value": record.getMessage(),
                        "mechanism": {"handled": True, "type": "logging"},
                    }
                ]
                props["$exception_fingerprint"] = fingerprint
                self._client.capture(
                    "$exception", distinct_id=distinct_id, properties=props
                )
        except Exception:
            pass  # error reporting must never break the pipeline
        finally:
            self._guard.active = False


def _build_posthog_handler() -> Optional[_PosthogErrorHandler]:
    """Return a handler when ``POSTHOG_ERRORS_API_KEY`` is set and the optional
    ``posthog`` extra imports; otherwise ``None`` (fail-open)."""

    api_key = os.environ.get("POSTHOG_ERRORS_API_KEY")
    if not api_key:
        return None
    try:
        from posthog import Posthog

        host = os.environ.get("POSTHOG_HOST") or _DEFAULT_POSTHOG_HOST
        client = Posthog(api_key, host=host)
    except ImportError:
        logging.getLogger(__name__).warning(
            "POSTHOG_ERRORS_API_KEY is set but the posthog extra is not installed"
        )
        return None
    except Exception:
        logging.getLogger(__name__).warning(
            "POSTHOG_ERRORS_API_KEY is set but the PostHog client could not be "
            "created; PostHog error tracking is off",
            exc_info=True,
        )
        return None
    return _PosthogErrorHandler(client)


_INITIALIZED = False
# The PostHog client installed by the first init_logger call, so later guards
# can drain it too.
_POSTHOG_CLIENT: Any = None


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

    global _INITIALIZED, _POSTHOG_CLIENT

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

        posthog_handler = _build_posthog_handler()
        if posthog_handler is not None:
            posthog_handler.addFilter(_ContextFilter(repo, tool, rid))
            root.addHandler(posthog_handler)

        _POSTHOG_CLIENT = (
            posthog_handler._client if posthog_handler is not None else None
        )
        guard = LoggerGuard(rid, sentry_enabled, _POSTHOG_CLIENT)
        atexit.register(guard.flush)
        _INITIALIZED = True
        return guard

    # Already initialized: swap the context filter on the handlers this module
    # installed (the ones carrying a _ContextFilter) so the new
    # repo/tool/run_id apply going forward. Other handlers are left alone.
    new_filter = _ContextFilter(repo, tool, rid)
    for handler in logging.getLogger().handlers:
        existing = [f for f in handler.filters if isinstance(f, _ContextFilter)]
        if not existing:
            continue
        for old in existing:
            handler.removeFilter(old)
        handler.addFilter(new_filter)

    return LoggerGuard(rid, sentry_enabled, _POSTHOG_CLIENT)


__all__ = ["LoggerConfig", "LoggerGuard", "init_logger"]
