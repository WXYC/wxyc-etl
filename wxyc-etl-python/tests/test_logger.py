"""Tests for the wxyc_etl.logger Python helper."""

from __future__ import annotations

import io
import json
import logging
import os
import sys

import pytest

from wxyc_etl.logger import LoggerGuard, init_logger


@pytest.fixture(autouse=True)
def _reset_logging(monkeypatch):
    """Reset the global root logger and the module's init flag between tests so
    each test sees a clean handler chain."""

    monkeypatch.delenv("SENTRY_DSN", raising=False)

    root = logging.getLogger()
    saved_handlers = root.handlers[:]
    saved_level = root.level
    root.handlers.clear()

    import wxyc_etl.logger as mod

    monkeypatch.setattr(mod, "_INITIALIZED", False)

    yield

    root.handlers = saved_handlers
    root.setLevel(saved_level)


def _capture_handler_output(monkeypatch) -> io.StringIO:
    buf = io.StringIO()
    monkeypatch.setattr("sys.stderr", buf)
    return buf


def test_init_without_dsn_returns_guard(monkeypatch):
    buf = _capture_handler_output(monkeypatch)

    guard = init_logger(repo="test-repo", tool="test-tool")

    assert isinstance(guard, LoggerGuard)
    assert guard.run_id  # generated UUID

    logging.getLogger("wxyc.test").info("hello", extra={"step": "load"})

    line = buf.getvalue().strip().splitlines()[0]
    payload = json.loads(line)
    assert payload["repo"] == "test-repo"
    assert payload["tool"] == "test-tool"
    assert payload["step"] == "load"
    assert payload["run_id"] == guard.run_id
    assert payload["message"] == "hello"
    assert payload["level"] == "INFO"


def test_init_uses_provided_run_id(monkeypatch):
    _capture_handler_output(monkeypatch)
    guard = init_logger(repo="r", tool="t", run_id="fixed-run-1234")
    assert guard.run_id == "fixed-run-1234"


def test_init_idempotent_updates_tags(monkeypatch):
    buf = _capture_handler_output(monkeypatch)

    init_logger(repo="first", tool="first-tool", run_id="run-1")
    init_logger(repo="second", tool="second-tool", run_id="run-2")

    logging.getLogger("wxyc.test").info("after-second")
    line = buf.getvalue().strip().splitlines()[-1]
    payload = json.loads(line)
    assert payload["repo"] == "second"
    assert payload["tool"] == "second-tool"
    assert payload["run_id"] == "run-2"


def test_step_defaults_to_empty_when_missing(monkeypatch):
    buf = _capture_handler_output(monkeypatch)

    init_logger(repo="r", tool="t", run_id="run-x")
    logging.getLogger("wxyc.test").info("no-step")

    payload = json.loads(buf.getvalue().strip().splitlines()[0])
    assert payload["step"] == ""


@pytest.mark.parametrize("dsn", ["", "https://key@o0.ingest.sentry.io/1"])
def test_sentry_dsn_is_accepted_and_ignored(monkeypatch, dsn):
    """``sentry_dsn`` stays in the signature for callers that still pass it,
    but sentry_sdk is never imported."""

    monkeypatch.setitem(sys.modules, "sentry_sdk", None)
    monkeypatch.setenv("SENTRY_DSN", dsn)
    _capture_handler_output(monkeypatch)
    guard = init_logger(repo="r", tool="t", sentry_dsn=dsn)
    assert isinstance(guard, LoggerGuard)
    guard.flush()
