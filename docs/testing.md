# Testing

```bash
# Rust unit tests (350 in wxyc-etl/src/)
cargo test --lib

# Rust integration tests (in wxyc-etl/tests/)
cargo test --workspace

# Python binding tests (require an editable install of wxyc-etl-python)
cd wxyc-etl-python && pytest

# Wheel lifecycle test (build, install, import, basic smoke)
python tests/test_wheel_lifecycle.py
```

Notable test files:

- `wxyc-etl/tests/python_parity.rs` — locks Rust output against expected values produced by the legacy Python implementations in the consumer repos. Editing `text::normalize_artist_name` requires updating these expectations.
- `wxyc-etl/tests/identity_normalization_parity.rs` — locks the cross-cache-identity layered normalizer (`to_identity_match_form` and the opt-in step-6/step-8 variants) against `wxyc-etl/tests/fixtures/identity_normalization_cases.csv`. The CSV is the spec — Postgres analog `wxyc_identity_match_artist` (when it ships) targets the same outputs byte-for-byte. Updating the CSV requires re-deriving from `docs/normalization.md`, not from the Rust output.
- `wxyc-etl/tests/wxyc_unaccent_rules_test.rs` — generator + freshness check for `data/wxyc_unaccent.rules` (the Postgres-side counterpart to `strip_combining_selective` + `apply_folds`). Default mode asserts the committed rules + version files match what the generator would emit from `to_match_form`. Run with `WXYC_REGENERATE_RULES=1` to refresh after any `to_match_form` behavior change.
- `wxyc-etl/tests/postgres_parity_test.rs` — `#[ignore]`-gated Rust ↔ Postgres byte-equality check for the four identity-match entry points. Driven by `tests/fixtures/identity_normalization_cases.csv`. Local setup: `bash scripts/install_wxyc_unaccent.sh` to copy `data/wxyc_unaccent.rules` into `$SHAREDIR/tsearch_data/`; then `TEST_DATABASE_URL=... cargo test --test postgres_parity_test -- --include-ignored`. CI installs the rules with `docker cp` into the service container.
- `wxyc-etl/tests/regression_report.rs` — generates `target/regression-report.json` per plan §3.3.4. Run with `WXYC_REGRESSION_INPUT=/path/to/artists.csv cargo test --test regression_report -- --include-ignored`. Without the env var the harness still runs a self-test against the parity matrix to keep the JSON schema honest. Sections A + B (per-row diff, per-step impact) come from the input CSV alone; sections C + D (match-shift, per-step shift) are computed from in-memory partition analysis; section E (confidence threshold validation) is a stub — it requires joining `entity.identity.confidence` from a live Homebrew cache and is left for a follow-up PR or for an inline run.
- `wxyc-etl/tests/integration_modules.rs` — cross-module integration scenarios.
- `wxyc-etl/tests/pg_error_tests.rs` — pipeline error / panic propagation against PostgreSQL. Some tests gate on `TEST_DATABASE_URL`; others (`pipeline_error_tests::test_scanner_panic_does_not_deadlock_writer`) are flaky on local macOS but pass in CI.
- `wxyc-etl/tests/panic_recovery.rs` — verifies the pipeline runner doesn't deadlock when a worker panics.
- `wxyc-etl-python/tests/test_performance.py` — marked `@perf`, requires release-built wheel.

Test fixtures use canonical WXYC artist names from the org-level convention. Diacritic-bearing inputs are drawn from `wxycCanonicalArtistNames` in `@wxyc/shared` (Nilüfer Yanya for ü, Csillagrablók for ó, Hermanos Gutiérrez for é). Non-canonical inputs (`10,000 Maniacs`, `Andy Human and the Reptoids`, `Łona`, CJK strings) are kept only where they exercise specific algorithm guards or Unicode behaviors with no canonical analogue.
