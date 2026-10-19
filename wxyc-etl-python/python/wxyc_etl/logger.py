"""Structured JSON logging for WXYC ETL pipelines (Python side).

Mirrors :mod:`wxyc_etl::logger` (Rust). Every Python ETL should call
:func:`init_logger` once at startup with a ``repo`` and ``tool`` identifying
the pipeline. Logs emit one JSON object per line on the root logger with the
tags ``repo``, ``tool``, ``step``, and ``run_id`` (``step`` is set per-call via
:func:`logging.Logger.info` ``extra=`` or via :class:`LoggerStep`). When
``POSTHOG_ERRORS_API_KEY`` is set and the optional ``posthog`` extra is
installed, ERROR records are also sent to PostHog error tracking (see
:class:`_PosthogErrorHandler`). ``sentry_dsn`` is still accepted for callers
that pass it, but is ignored.

Usage::

    from wxyc_etl.logger import init_logger

    guard = init_logger(repo="discogs-etl", tool="discogs-etl daily-sync")

    import logging
    log = logging.getLogger(__name__)
    log.info("loaded recordings", extra={"step": "import", "rows": 42})

The returned :class:`LoggerGuard` flushes pending PostHog events
when its ``flush`` method is called (or when the program exits).
"""

from __future__ import annotations

import atexit
import logging
import os
import re
import sys
import threading
import time
import uuid
from dataclasses import dataclass
from typing import Any, Callable, Optional

# Budget from the error-tracking contract (WXYC/wiki patterns/error-tracking.md,
# section 5). A scheduled run gets a fixed per-run ceiling and no refill; a
# long-running process gets a small burst plus a refill that spends a daily
# ceiling.
POSTHOG_ERRORS_BUCKET_CAPACITY = 10
POSTHOG_ERRORS_REFILL_PER_SECOND = 0.0
POSTHOG_ERRORS_LONG_RUNNING_CAPACITY = 20
_SECONDS_PER_DAY = 86400
# posthog 7.65.0+ sets $exception_level itself and warns when it is overridden.
_POSTHOG_SETS_LEVEL_FROM = (7, 65, 0)
_DEFAULT_ENVIRONMENT = "production"
# The contract allows exactly production, staging and development; common
# aliases are mapped onto them. Unknown values pass through with a warning.
_ENVIRONMENT_ALIASES = {
    "production": "production",
    "prod": "production",
    "staging": "staging",
    "stage": "staging",
    "development": "development",
    "dev": "development",
    "local": "development",
}
_WARNED_ENVIRONMENTS: set = set()
POSTHOG_ERRORS_DEDUPE_WINDOW_SECONDS = 60.0
_MAX_TRACKED_FINGERPRINTS = 512
_DEFAULT_POSTHOG_HOST = "https://us.i.posthog.com"
# Loggers whose output must never be re-captured (the SDK logs through these).
_POSTHOG_SKIP_LOGGERS = ("posthog", "backoff", "wxyc_etl")


@dataclass(frozen=True)
class LoggerConfig:
    """Configuration for :func:`init_logger`. ``sentry_dsn`` is accepted and
    ignored; ``run_id=None`` generates a UUIDv4."""

    repo: str
    tool: str
    sentry_dsn: Optional[str] = None
    run_id: Optional[str] = None
    service: Optional[str] = None
    environment: Optional[str] = None
    posthog_run_ceiling: int = POSTHOG_ERRORS_BUCKET_CAPACITY
    posthog_daily_ceiling: Optional[int] = None


class LoggerGuard:
    """Returned by :func:`init_logger`. Call :meth:`flush` (or rely on the
    ``atexit`` hook) to drain the PostHog errors client before the process
    exits.
    """

    def __init__(self, run_id: str, posthog_client: Any = None) -> None:
        self.run_id = run_id
        self._posthog_client = posthog_client

    def flush(self, timeout: float = 2.0) -> None:
        if self._posthog_client is not None:
            try:
                self._posthog_client.flush()
            except Exception:  # pragma: no cover - flush is best-effort
                pass


class _ContextFilter(logging.Filter):
    """Inject repo/tool/run_id/service/environment onto every record so the JSON formatter emits
    them. Per-event ``step`` is supplied by callers via ``extra={"step": ...}``
    and survives this filter unchanged.
    """

    def __init__(
        self, repo: str, tool: str, run_id: str, service: str, environment: str
    ) -> None:
        super().__init__()
        self._repo = repo
        self._tool = tool
        self._run_id = run_id
        self._service = service
        self._environment = environment

    def filter(self, record: logging.LogRecord) -> bool:
        if not hasattr(record, "repo"):
            record.repo = self._repo
        if not hasattr(record, "tool"):
            record.tool = self._tool
        if not hasattr(record, "run_id"):
            record.run_id = self._run_id
        if not hasattr(record, "service"):
            record.service = self._service
        if not hasattr(record, "environment"):
            record.environment = self._environment
        if not hasattr(record, "step"):
            record.step = ""
        return True


_CAPTURED_ATTR = "_wxyc_etl_posthog_captured"


class _PosthogErrorHandler(logging.Handler):
    """Send ERROR records to PostHog error tracking.

    Records with ``exc_info`` go through ``capture_exception``; the rest become
    a synthesized ``$exception`` event fingerprinted by logger name and
    unformatted message template, joined with ``:`` into the single string
    PostHog expects for ``$exception_fingerprint``. Events follow the
    error-tracking contract: ``distinct_id`` is the ``service`` value and every
    event carries ``service``, ``environment`` and ``logger``. A same-fingerprint
    dedupe window and a token bucket bound the volume so an error loop cannot
    flood the project. ``set_exception_level`` adds ``$exception_level`` to
    ``capture_exception`` calls, for posthog SDKs older than 7.65.0.
    """

    def __init__(
        self,
        client: Any,
        clock: Callable[[], float] = time.monotonic,
        capacity: float = POSTHOG_ERRORS_BUCKET_CAPACITY,
        refill_per_second: float = POSTHOG_ERRORS_REFILL_PER_SECOND,
        dedupe_window: float = POSTHOG_ERRORS_DEDUPE_WINDOW_SECONDS,
        set_exception_level: bool = False,
    ) -> None:
        super().__init__(level=logging.ERROR)
        self._client = client
        self._clock = clock
        self._capacity = capacity
        self._refill = refill_per_second
        self._window = dedupe_window
        self._set_exception_level = set_exception_level
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
        # A record the gate drops leaves the exception unmarked. A dedupe drop
        # must not hide a later crash: the hook's record has another fingerprint
        # and would be sent. A token-bucket drop needs no special handling: the
        # hook's record would hit the same empty bucket anyway.
        if not self._admit((fingerprint, exc_type)):
            return
        props = {
            key: getattr(record, key, "")
            for key in ("repo", "tool", "run_id", "step", "service", "environment")
        }
        props["logger"] = record.name
        props["$process_person_profile"] = False
        distinct_id = props["service"]
        self._guard.active = True
        try:
            if record.exc_info and record.exc_info[0] is not None:
                if self._set_exception_level:
                    props["$exception_level"] = "error"
                extra = {}
                mechanism = getattr(record, "mechanism", None)
                # posthog-python reads _capture_metadata from 7.65.0; 6.9 through
                # 7.64 ignore it and send the crash as handled.
                if mechanism:
                    extra["_capture_metadata"] = {"mechanism": mechanism}
                self._client.capture_exception(
                    record.exc_info, distinct_id=distinct_id, properties=props, **extra
                )
                try:  # lets the excepthooks skip an exception that was logged first
                    setattr(record.exc_info[1], _CAPTURED_ATTR, True)
                except Exception:
                    pass
            else:
                props["$exception_list"] = [
                    {
                        "type": "Message",
                        "value": record.getMessage(),
                        "mechanism": {
                            "type": "generic",
                            "handled": True,
                            "synthetic": True,
                        },
                    }
                ]
                props["$exception_level"] = "error"
                props["$exception_fingerprint"] = fingerprint
                self._client.capture(
                    "$exception", distinct_id=distinct_id, properties=props
                )
        except Exception:
            pass  # error reporting must never break the pipeline
        finally:
            self._guard.active = False


def _install_excepthooks(handler: _PosthogErrorHandler) -> None:
    """Chain ``sys.excepthook`` and ``threading.excepthook`` so an uncaught
    exception goes through the handler's dedupe window and token bucket, the
    client is flushed, and the previous hook always runs. Crashes are sent
    with ``mechanism.handled=false``. An exception the handler already
    captured from a log record is not sent twice. ``KeyboardInterrupt`` and
    ``SystemExit`` (``SystemExit`` only in threads) are not reported."""

    def report(exc_type, exc, tb, mechanism_type, skip, previous, *previous_args):
        try:
            if not issubclass(exc_type, skip):
                try:
                    if not getattr(exc, _CAPTURED_ATTR, False):
                        record = logging.LogRecord(
                            "uncaught", logging.ERROR, __file__, 0,
                            "uncaught exception", (), (exc_type, exc, tb),
                        )
                        record.mechanism = {"type": mechanism_type, "handled": False}
                        handler.handle(record)
                    handler._client.flush()
                except Exception:
                    pass  # error reporting must never mask the original traceback
        finally:
            previous(*previous_args)

    previous_sys = sys.excepthook
    previous_thread = threading.excepthook

    def sys_hook(exc_type, exc, tb):
        report(exc_type, exc, tb, "excepthook", (KeyboardInterrupt, SystemExit),
               previous_sys, exc_type, exc, tb)

    def thread_hook(args):
        report(args.exc_type, args.exc_value, args.exc_traceback,
               "threading.excepthook", (SystemExit,), previous_thread, args)

    sys.excepthook = sys_hook
    threading.excepthook = thread_hook


def _posthog_older_than_level_support() -> bool:
    """True when the installed posthog predates 7.65.0 (or its version cannot
    be read), so the handler must set ``$exception_level`` itself."""

    try:
        from importlib.metadata import version

        raw = version("posthog")
    except Exception:
        return True
    # Leading integers of each dot part, so "7.65.0rc1" reads as (7, 65, 0): a
    # pre-release of 7.65 already sets the level itself.
    parts = []
    for part in raw.split(".")[:3]:
        match = re.match(r"\d+", part)
        if match is None:
            break
        parts.append(int(match.group()))
    if not parts:
        return True
    parts += [0] * (3 - len(parts))
    return tuple(parts) < _POSTHOG_SETS_LEVEL_FROM


def _normalize_environment(value: str) -> str:
    """Map ``value`` onto production, staging or development (case and
    whitespace insensitive, with common aliases). An unknown value passes
    through unchanged and logs a warning once."""

    key = value.strip().lower()
    mapped = _ENVIRONMENT_ALIASES.get(key)
    if mapped is not None:
        return mapped
    if value not in _WARNED_ENVIRONMENTS:
        _WARNED_ENVIRONMENTS.add(value)
        logging.getLogger(__name__).warning(
            "environment %r is not one of production, staging or development "
            "(error-tracking contract); passing it through",
            value,
        )
    return value


def _build_posthog_handler(
    run_ceiling: int = POSTHOG_ERRORS_BUCKET_CAPACITY,
    daily_ceiling: Optional[int] = None,
) -> Optional[_PosthogErrorHandler]:
    """Return a handler when ``POSTHOG_ERRORS_API_KEY`` is set and the optional
    ``posthog`` extra imports; otherwise ``None`` (fail-open). ``daily_ceiling``
    selects the long-running bucket; otherwise ``run_ceiling`` is a fixed
    per-run budget."""

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
    if daily_ceiling is not None:
        capacity = POSTHOG_ERRORS_LONG_RUNNING_CAPACITY
        refill = daily_ceiling / _SECONDS_PER_DAY
    else:
        capacity, refill = run_ceiling, POSTHOG_ERRORS_REFILL_PER_SECOND
    return _PosthogErrorHandler(
        client,
        capacity=capacity,
        refill_per_second=refill,
        set_exception_level=_posthog_older_than_level_support(),
    )


_INITIALIZED = False
# The PostHog client installed by the first init_logger call, so later guards
# can drain it too.
_POSTHOG_CLIENT: Any = None
# The (run, daily) ceilings the handler was built with; None until a handler is built.
_POSTHOG_BUDGET: Optional[tuple] = None


def init_logger(
    repo: str,
    tool: str,
    sentry_dsn: Optional[str] = None,
    run_id: Optional[str] = None,
    level: int = logging.INFO,
    posthog_errors: bool = True,
    service: Optional[str] = None,
    environment: Optional[str] = None,
    posthog_run_ceiling: int = POSTHOG_ERRORS_BUCKET_CAPACITY,
    posthog_daily_ceiling: Optional[int] = None,
) -> LoggerGuard:
    """Initialize JSON logging on the root logger. Idempotent: subsequent calls
    update tags but don't double-install handlers. ``sentry_dsn`` is accepted
    for backward compatibility and ignored.

    ``posthog_errors=False`` skips the PostHog errors handler and client, for
    a process that also runs wxyc-fastapi's ``init_error_tracking``. As with
    the other handlers, the first call's choice wins. When the handler is
    built, ``sys.excepthook`` and ``threading.excepthook`` are also chained so
    uncaught exceptions are reported through the same gate.

    ``service`` (default: ``tool`` in kebab-case) is the PostHog ``distinct_id``
    and a tag on every record; ``environment`` defaults to the
    ``WXYC_ENVIRONMENT`` env var, else ``production``. The PostHog budget
    defaults to a scheduled run: ``posthog_run_ceiling`` events (10; pass 2 for
    a job that runs more than 4 times a day) and no refill. A long-running
    process must pass ``posthog_daily_ceiling`` instead, which selects a
    capacity-20 bucket refilling at ceiling / 86,400 per second. The handler
    is built once, on the first call: ceilings passed on later calls are
    ignored (a different pair logs a warning). ``environment`` is normalised to
    production, staging or development (``prod``, ``stage``, ``dev`` and
    ``local`` are aliases); an unknown value passes through with a warning.
    """

    global _INITIALIZED, _POSTHOG_CLIENT, _POSTHOG_BUDGET

    rid = run_id or str(uuid.uuid4())
    svc = service or "-".join(tool.lower().split())
    env = _normalize_environment(
        environment or os.environ.get("WXYC_ENVIRONMENT") or _DEFAULT_ENVIRONMENT
    )

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
            "%(repo)s %(tool)s %(step)s %(run_id)s %(service)s %(environment)s",
            rename_fields={"asctime": "timestamp", "levelname": "level"},
        )

        handler = logging.StreamHandler(sys.stderr)
        handler.setFormatter(formatter)
        handler.addFilter(_ContextFilter(repo, tool, rid, svc, env))

        root = logging.getLogger()
        root.addHandler(handler)
        root.setLevel(level)

        posthog_handler = (
            _build_posthog_handler(posthog_run_ceiling, posthog_daily_ceiling)
            if posthog_errors
            else None
        )
        if posthog_handler is not None:
            _POSTHOG_BUDGET = (posthog_run_ceiling, posthog_daily_ceiling)
            posthog_handler.addFilter(_ContextFilter(repo, tool, rid, svc, env))
            root.addHandler(posthog_handler)
            _install_excepthooks(posthog_handler)

        _POSTHOG_CLIENT = (
            posthog_handler._client if posthog_handler is not None else None
        )
        guard = LoggerGuard(rid, _POSTHOG_CLIENT)
        atexit.register(guard.flush)
        _INITIALIZED = True
        return guard

    if (
        posthog_errors
        and _POSTHOG_BUDGET is not None
        and _POSTHOG_BUDGET != (posthog_run_ceiling, posthog_daily_ceiling)
    ):
        logging.getLogger(__name__).warning(
            "init_logger was already called with PostHog ceilings %r; ignoring "
            "run/daily ceilings (%r, %r) because the handler is built once",
            _POSTHOG_BUDGET,
            posthog_run_ceiling,
            posthog_daily_ceiling,
        )

    # Already initialized: swap the context filter on the handlers this module
    # installed (the ones carrying a _ContextFilter) so the new
    # repo/tool/run_id apply going forward. Other handlers are left alone.
    new_filter = _ContextFilter(repo, tool, rid, svc, env)
    for handler in logging.getLogger().handlers:
        existing = [f for f in handler.filters if isinstance(f, _ContextFilter)]
        if not existing:
            continue
        for old in existing:
            handler.removeFilter(old)
        handler.addFilter(new_filter)

    return LoggerGuard(rid, _POSTHOG_CLIENT)


__all__ = ["LoggerConfig", "LoggerGuard", "init_logger"]
