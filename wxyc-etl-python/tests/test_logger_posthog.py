"""Tests for the PostHog ERROR-to-exception handler in wxyc_etl.logger.

The ``posthog`` SDK is optional, so these tests never import it: handler tests
inject a ``MagicMock`` client and ``init_logger`` tests stub the module in
``sys.modules``.
"""

from __future__ import annotations

import logging
import sys
import types
from unittest.mock import MagicMock

import pytest

import wxyc_etl.logger as mod
from wxyc_etl.logger import init_logger


class FakeClock:
    def __init__(self) -> None:
        self.now = 0.0

    def __call__(self) -> float:
        return self.now

    def advance(self, seconds: float) -> None:
        self.now += seconds


@pytest.fixture(autouse=True)
def _reset_logging(monkeypatch):
    monkeypatch.delenv("SENTRY_DSN", raising=False)
    monkeypatch.delenv("POSTHOG_ERRORS_API_KEY", raising=False)
    monkeypatch.delenv("POSTHOG_HOST", raising=False)

    root = logging.getLogger()
    saved_handlers = root.handlers[:]
    saved_level = root.level
    root.handlers.clear()
    monkeypatch.setattr(mod, "_INITIALIZED", False)

    yield

    root.handlers = saved_handlers
    root.setLevel(saved_level)


def _record(
    msg: str = "boom",
    args: tuple = (),
    *,
    name: str = "etl.lookup",
    level: int = logging.ERROR,
    exc_info=None,
) -> logging.LogRecord:
    record = logging.LogRecord(name, level, __file__, 1, msg, args, exc_info)
    record.repo, record.tool, record.run_id, record.step = "r", "t", "run-1", "import"
    return record


def _exc_info():
    try:
        raise ValueError("kaboom")
    except ValueError:
        return sys.exc_info()


def _handler(client: MagicMock, clock: FakeClock | None = None, **kwargs):
    return mod._PosthogErrorHandler(client, clock=clock or FakeClock(), **kwargs)


def _sent(client: MagicMock) -> int:
    return client.capture.call_count + client.capture_exception.call_count


def test_exc_info_record_goes_through_capture_exception():
    client = MagicMock()
    exc_info = _exc_info()

    _handler(client).handle(_record(exc_info=exc_info))

    client.capture.assert_not_called()
    args, kwargs = client.capture_exception.call_args
    assert args == (exc_info,)
    assert kwargs["properties"]["repo"] == "r"
    assert kwargs["properties"]["tool"] == "t"
    assert kwargs["properties"]["run_id"] == "run-1"


def test_record_without_exc_info_synthesizes_exception_event():
    client = MagicMock()

    _handler(client).handle(_record("lookup failed for %s", ("Juana Molina",)))

    client.capture_exception.assert_not_called()
    args, kwargs = client.capture.call_args
    assert args == ("$exception",)
    props = kwargs["properties"]
    assert props["$exception_list"] == [
        {
            "type": "etl.lookup",
            "value": "lookup failed for Juana Molina",
            "mechanism": {"handled": True, "type": "logging"},
        }
    ]
    assert props["$exception_fingerprint"] == ["etl.lookup", "lookup failed for %s"]
    assert (props["repo"], props["tool"], props["run_id"], props["step"]) == ("r", "t", "run-1", "import")


def test_levels_below_error_are_ignored():
    client = MagicMock()
    logger = logging.getLogger("etl.levels")
    logger.propagate = False
    logger.addHandler(_handler(client))
    logger.warning("not an error")
    logger.error("an error")
    assert _sent(client) == 1


@pytest.mark.parametrize("name", ["posthog", "posthog.client", "backoff", "wxyc_etl.logger"])
def test_own_logger_output_is_not_captured(name):
    client = MagicMock()
    _handler(client).handle(_record(name=name))
    assert _sent(client) == 0


def test_error_logged_from_inside_capture_does_not_recurse():
    client = MagicMock()
    logger = logging.getLogger("etl.reentrant")
    logger.propagate = False
    logger.addHandler(_handler(client))
    client.capture.side_effect = lambda *a, **kw: logger.error("sdk hiccup")

    logger.error("first")

    assert client.capture.call_count == 1


def test_sdk_failure_is_swallowed():
    client = MagicMock()
    client.capture.side_effect = RuntimeError("sdk down")
    client.capture_exception.side_effect = RuntimeError("sdk down")
    handler = _handler(client)
    handler.handle(_record("a"))
    handler.handle(_record("b", exc_info=_exc_info()))


def test_bucket_drops_beyond_capacity_across_both_paths():
    client = MagicMock()
    handler = _handler(client, capacity=2, refill_per_second=1)

    handler.handle(_record("a"))
    handler.handle(_record("b", exc_info=_exc_info()))
    handler.handle(_record("c"))

    assert _sent(client) == 2


def test_bucket_refills_over_time():
    client = MagicMock()
    clock = FakeClock()
    handler = _handler(client, clock, capacity=1, refill_per_second=1)

    handler.handle(_record("a"))
    handler.handle(_record("b"))  # dropped
    clock.advance(1.0)
    handler.handle(_record("c"))

    assert _sent(client) == 2


def test_deduped_records_do_not_consume_tokens():
    client = MagicMock()
    handler = _handler(client, capacity=2, refill_per_second=1)

    handler.handle(_record("same"))
    handler.handle(_record("same"))
    handler.handle(_record("other"))

    assert _sent(client) == 2


def test_same_fingerprint_dedupes_inside_window_only():
    client = MagicMock()
    clock = FakeClock()
    handler = _handler(client, clock)

    handler.handle(_record("same %s", ("1",)))
    clock.advance(mod.POSTHOG_ERRORS_DEDUPE_WINDOW_SECONDS - 1)
    handler.handle(_record("same %s", ("2",)))
    assert _sent(client) == 1

    clock.advance(2)
    handler.handle(_record("same %s", ("3",)))
    assert _sent(client) == 2


def test_dedupe_distinguishes_logger_template_and_exception_type():
    client = MagicMock()
    handler = _handler(client)
    try:
        raise KeyError("k")
    except KeyError:
        key_error = sys.exc_info()

    handler.handle(_record("same", name="etl.a"))
    handler.handle(_record("same", name="etl.b"))
    handler.handle(_record("different", name="etl.a"))
    handler.handle(_record("same", exc_info=_exc_info()))
    handler.handle(_record("same", exc_info=key_error))

    assert _sent(client) == 5


def test_seen_table_stays_bounded():
    client = MagicMock()
    clock = FakeClock()
    handler = _handler(client, clock, capacity=1e9, refill_per_second=1e9)
    for i in range(mod._MAX_TRACKED_FINGERPRINTS + 50):
        handler.handle(_record(f"m{i}"))
        clock.advance(0.001)
    assert len(handler._seen) <= mod._MAX_TRACKED_FINGERPRINTS


def _stub_posthog(monkeypatch) -> MagicMock:
    cls = MagicMock(name="Posthog")
    monkeypatch.setitem(sys.modules, "posthog", types.SimpleNamespace(Posthog=cls))
    return cls


def test_init_installs_handler_when_key_set(monkeypatch):
    monkeypatch.setenv("POSTHOG_ERRORS_API_KEY", "phc_errors")
    cls = _stub_posthog(monkeypatch)

    init_logger(repo="r", tool="t", run_id="run-1")

    args, kwargs = cls.call_args
    assert args == ("phc_errors",)
    assert kwargs["host"] == "https://us.i.posthog.com"
    handlers = [h for h in logging.getLogger().handlers if isinstance(h, mod._PosthogErrorHandler)]
    assert len(handlers) == 1

    # Feed the handler directly: a record with no tags proves its own filter injects them.
    handlers[0].handle(logging.LogRecord("etl.lookup", logging.ERROR, __file__, 1, "lookup failed", (), None))
    sent = cls.return_value.capture.call_args.kwargs["properties"]
    assert (sent["repo"], sent["tool"], sent["run_id"]) == ("r", "t", "run-1")


def test_init_honors_posthog_host(monkeypatch):
    monkeypatch.setenv("POSTHOG_ERRORS_API_KEY", "phc_errors")
    monkeypatch.setenv("POSTHOG_HOST", "https://eu.i.posthog.com")
    cls = _stub_posthog(monkeypatch)

    init_logger(repo="r", tool="t")

    assert cls.call_args.kwargs["host"] == "https://eu.i.posthog.com"


def test_init_without_key_installs_nothing(monkeypatch):
    cls = _stub_posthog(monkeypatch)

    init_logger(repo="r", tool="t")

    cls.assert_not_called()
    assert not any(isinstance(h, mod._PosthogErrorHandler) for h in logging.getLogger().handlers)


def test_init_without_posthog_extra_fails_open(monkeypatch):
    monkeypatch.setenv("POSTHOG_ERRORS_API_KEY", "phc_errors")
    monkeypatch.setitem(sys.modules, "posthog", None)

    init_logger(repo="r", tool="t")

    assert not any(isinstance(h, mod._PosthogErrorHandler) for h in logging.getLogger().handlers)
    logging.getLogger("etl.lookup").error("still logs")  # must not raise


def test_init_is_idempotent_for_the_handler(monkeypatch):
    monkeypatch.setenv("POSTHOG_ERRORS_API_KEY", "phc_errors")
    _stub_posthog(monkeypatch)

    init_logger(repo="r", tool="t")
    init_logger(repo="r2", tool="t2")

    handlers = [h for h in logging.getLogger().handlers if isinstance(h, mod._PosthogErrorHandler)]
    assert len(handlers) == 1


def test_guard_flush_drains_posthog_client(monkeypatch):
    monkeypatch.setenv("POSTHOG_ERRORS_API_KEY", "phc_errors")
    cls = _stub_posthog(monkeypatch)

    init_logger(repo="r", tool="t").flush()

    cls.return_value.flush.assert_called_once()
