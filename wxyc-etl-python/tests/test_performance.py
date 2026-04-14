"""Performance benchmarks for batch operations.

These tests validate that Rust-backed batch operations achieve the expected
throughput. They are designed for release builds (`maturin develop --release`);
debug builds will be significantly slower.
"""

import time

import pytest
from wxyc_etl import text

# Performance tests require release build; mark so they can be skipped in CI debug
perf = pytest.mark.perf


@perf
def test_batch_normalize_performance():
    """100K normalizations should complete in under 200ms (release build)."""
    names = ["Björk"] * 100_000
    start = time.time()
    results = text.batch_normalize(names)
    elapsed = time.time() - start
    assert elapsed < 0.2, f"batch_normalize took {elapsed:.3f}s for 100K names"
    assert len(results) == 100_000


@perf
def test_normalize_individual_calls():
    """100K individual normalize calls should complete in under 500ms."""
    start = time.time()
    for _ in range(100_000):
        text.normalize_artist_name("Björk")
    elapsed = time.time() - start
    assert elapsed < 0.5, f"100K normalize calls took {elapsed:.3f}s"


@perf
def test_batch_normalize_million():
    """1M normalizations via batch should complete in under 2s (release build)."""
    names = ["Sigur Rós"] * 1_000_000
    start = time.time()
    results = text.batch_normalize(names)
    elapsed = time.time() - start
    assert elapsed < 2.0, f"batch_normalize took {elapsed:.3f}s for 1M names"
    assert len(results) == 1_000_000
