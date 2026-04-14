"""Tests for wxyc_etl.state bindings (PipelineState)."""

import os
import tempfile

import pytest
from wxyc_etl.state import PipelineState


def test_pipeline_state_basic():
    state = PipelineState("postgresql:///discogs", "/tmp/csv", ["step1", "step2"])
    assert state.step_status("step1") == "pending"
    assert not state.is_completed("step1")

    state.mark_completed("step1")
    assert state.is_completed("step1")
    assert state.step_status("step1") == "completed"
    assert not state.is_completed("step2")


def test_pipeline_state_failed():
    state = PipelineState("db_url", "csv_dir", ["step1"])
    state.mark_failed("step1", "connection refused")
    assert state.step_status("step1") == "failed"
    assert state.step_error("step1") == "connection refused"


def test_pipeline_state_error_none_for_pending():
    state = PipelineState("db_url", "csv_dir", ["step1"])
    assert state.step_error("step1") is None


def test_pipeline_state_save_load():
    with tempfile.TemporaryDirectory() as d:
        path = os.path.join(d, "state.json")
        state = PipelineState("db_url", "csv_dir", ["step1"])
        state.mark_completed("step1")
        state.save(path)

        loaded = PipelineState.load(path)
        assert loaded.is_completed("step1")


def test_pipeline_state_save_load_with_failed():
    with tempfile.TemporaryDirectory() as d:
        path = os.path.join(d, "state.json")
        state = PipelineState("db", "csv", ["step1", "step2"])
        state.mark_completed("step1")
        state.mark_failed("step2", "disk full")
        state.save(path)

        loaded = PipelineState.load(path)
        assert loaded.is_completed("step1")
        assert loaded.step_status("step2") == "failed"
        assert loaded.step_error("step2") == "disk full"


def test_pipeline_state_validate_resume_ok():
    state = PipelineState("db_url", "csv_dir", ["step1"])
    state.validate_resume("db_url", "csv_dir")  # should not raise


def test_pipeline_state_validate_resume_db_mismatch():
    state = PipelineState("db_url", "csv_dir", ["step1"])
    with pytest.raises(ValueError):
        state.validate_resume("other_url", "csv_dir")


def test_pipeline_state_validate_resume_csv_mismatch():
    state = PipelineState("db_url", "csv_dir", ["step1"])
    with pytest.raises(ValueError):
        state.validate_resume("db_url", "other_csv")


def test_pipeline_state_unknown_step():
    state = PipelineState("db_url", "csv_dir", ["step1"])
    assert state.step_status("nonexistent") == "unknown"
