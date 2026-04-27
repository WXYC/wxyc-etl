# wxyc-etl

Shared Rust crate (with PyO3 Python bindings) that supplies the cross-cutting primitives used by every WXYC ETL repo: text normalization, fuzzy matching, PostgreSQL bulk loading, a parallel pipeline framework, a MySQL-dump parser, schema constants, and JSON state tracking.

## Workspace Layout

Cargo workspace with two members:

| Crate | Path | Purpose |
|---|---|---|
| `wxyc-etl` | `wxyc-etl/` | Pure Rust library. Used by other Rust ETL repos (discogs-xml-converter, musicbrainz-cache, wikidata-json-filter) via `[dependencies] wxyc-etl = { path = "../wxyc-etl" }`. |
| `wxyc-etl-python` | `wxyc-etl-python/` | PyO3 extension that re-exports a curated subset to Python as `wxyc_etl.text`, `wxyc_etl.fuzzy`, `wxyc_etl.parser`, `wxyc_etl.state`, `wxyc_etl.import_utils`, `wxyc_etl.schema`. Built with maturin. |

The Python crate lives in the same workspace so it shares the lockfile with the Rust crate it wraps; this guarantees the bindings always match the underlying library at the byte level.

## Modules (`wxyc-etl/src/`)

| Module | Purpose |
|---|---|
| `text` | NFKD-based artist-name normalization (`normalize_artist_name`, `strip_diacritics`, `normalize_title`); compilation detection (`is_compilation_artist`); multi-artist split with optional contextual hints (`split_artist_name`, `split_artist_name_contextual`); batch variants in `text::batch`; file-backed `ArtistFilter` / `TitleFilter` in `text::filter`. |
| `pg` | `BatchCopier` and friends for `COPY TEXT` bulk loading, FK-ordered flush, dedup tracking, admin helpers (`SET UNLOGGED` / `SET LOGGED`). Backed by sync `postgres` crate. |
| `pipeline` | Generic scanner → rayon → writer parallel framework (`PipelineRunner`, `PipelineOutput` trait). Used by the streaming Rust filters to pipe huge dumps through worker pools without building intermediate vectors. |
| `csv_writer` | `MultiCsvWriter` — write to many CSVs in parallel from a single producer. |
| `sqlite` | SQLite helpers: FTS5 setup, performance pragmas, batch insert wrappers. |
| `state` | `PipelineState` — JSON state file at the workspace root, tracks completed pipeline steps for `--resume`. `state::introspect` derives a state file from DB introspection (schema present, row counts, index presence) when no file exists. |
| `import` | Artist/track dedup helpers and column mapping for the discogs-etl import path. |
| `schema` | Table-name constants for the consumer databases (`schema::library`, `schema::discogs`, `schema::musicbrainz`, `schema::wikidata`, `schema::entity`). Single source of truth so consumer Rust code never hard-codes table names. |
| `fuzzy` | `LibraryIndex` (token-set + Jaro-Winkler scoring), batch filter via normalize + set lookup, classification metrics. |
| `parser` | Streaming MySQL `INSERT INTO ... VALUES (...)` tuple parser used to read tubafrenzy SQL dumps without loading them into memory. |
| `logger` | Sentry + structured JSON logging (`tracing` + `tracing-subscriber` JSON). See **Observability** below. |
| `cli` | Shared `clap` argument groups (`DatabaseArgs`, `ResumableBuildArgs`, `ImportArgs`) and `resolve_database_url(args, env_name)` for the cache-builder CLI convention. See **Cache-builder CLI convention** below. |

## Observability

Every ETL binary should call `wxyc_etl::logger::init` (Rust) or `wxyc_etl.logger.init_logger` (Python) once at startup. Both emit one JSON object per line on stderr with four required tags so logs are uniformly aggregatable across pipelines:

| Tag | Source | Example |
|---|---|---|
| `repo` | passed to `init` | `"musicbrainz-cache"` |
| `tool` | passed to `init` | `"musicbrainz-cache build"` |
| `step` | per-event field | `"import"`, `"resolve"`, `"copy"` |
| `run_id` | UUIDv4 (or override) | `"4eb6f1b7-..."` |

When the `SENTRY_DSN` env var is set (or `sentry_dsn` is passed in), panics and `tracing::error!` events (Rust) / `logger.error` events (Python) are forwarded to Sentry tagged with the same fields.

### Rust

```rust
use wxyc_etl::logger::{self, LoggerConfig};

fn main() {
    let _guard = logger::init(LoggerConfig {
        repo: "musicbrainz-cache",
        tool: "musicbrainz-cache build",
        sentry_dsn: None,         // falls back to SENTRY_DSN env
        run_id: None,             // generates UUIDv4
    });

    tracing::info_span!("import", step = "import").in_scope(|| {
        tracing::info!(rows = 42, "loaded recordings");
    });
}
```

The guard must be held for the lifetime of `main` so Sentry flushes on drop.

### Python

```python
from wxyc_etl.logger import init_logger
import logging

guard = init_logger(repo="discogs-etl", tool="discogs-etl daily-sync")

log = logging.getLogger(__name__)
log.info("loaded recordings", extra={"step": "import", "rows": 42})
```

`init_logger` is idempotent — calling it again replaces the tag values without re-installing handlers. Drop the guard or call `guard.flush()` to drain Sentry before exit (also registered via `atexit`).

`lib.rs` re-exports each module unchanged: consumers do `use wxyc_etl::text::normalize_artist_name`.

## Cache-builder CLI convention

Every cache builder (`musicbrainz-cache`, `wikidata-cache`, `discogs-xml-converter`, `discogs-etl`) is shaped the same way:

| Subcommand | Flags | Purpose |
|---|---|---|
| `<tool> build` | `--database-url`, `--resume`, `--state-file`, `--data-dir` | populate / refresh the cache (resumable) |
| `<tool> import` | `--database-url`, `--fresh`, `--data-dir` | load a fresh dump |

Two rules per builder:

1. **`--database-url` is required, with an env fallback.** When the flag is absent, fall back to `DATABASE_URL_<UPPERCASE_NAME>` (e.g. `DATABASE_URL_MUSICBRAINZ`, `DATABASE_URL_WIKIDATA`). Use [`wxyc_etl::cli::resolve_database_url`] to enforce this:

   ```rust
   use clap::Parser;
   use wxyc_etl::cli::{resolve_database_url, DatabaseArgs, ResumableBuildArgs};

   #[derive(Parser)]
   struct BuildCli {
       #[command(flatten)]
       db: DatabaseArgs,
       #[command(flatten)]
       build: ResumableBuildArgs,
   }

   let cli = BuildCli::parse();
   let url = resolve_database_url(&cli.db, "DATABASE_URL_MUSICBRAINZ")?;
   ```

2. **Compose, don't redeclare.** Use `#[command(flatten)]` rather than copy-pasting flag definitions; the shared module is the source of truth.

## Python Bindings (`wxyc-etl-python/`)

Mixed Rust/Python package laid out as:

```
wxyc-etl-python/
  src/                  # Rust extension, built as wxyc_etl._native
  python/wxyc_etl/      # pure-Python helpers
    __init__.py         # re-exports the Rust submodules
    logger.py           # Sentry + JSON logging (pure Python)
```

`pyproject.toml` has `python-source = "python"` and `module-name = "wxyc_etl._native"` so maturin packs both halves into a single `wxyc_etl` package. The Rust crate registers its submodules under `wxyc_etl.<name>` in `sys.modules` so consumers can use either `from wxyc_etl.text import ...` or `from wxyc_etl import text`. Pure-Python additions go in `python/wxyc_etl/*.py`.

Currently exposed Rust submodules: `text`, `parser`, `state`, `import_utils`, `schema`, `fuzzy`. The `pg`, `pipeline`, `csv_writer`, and `sqlite` modules are not exposed — Python consumers don't need them (they have psycopg/asyncpg directly).

Pure-Python helpers: `logger`.

### Install for downstream Python repos

From the consuming repo (e.g. discogs-etl, semantic-index):

```bash
pip install -e ../wxyc-etl/wxyc-etl-python
```

The `-e` editable install rebuilds the wheel via maturin on every `pip install`, so changes to the Rust source are picked up the next time you reinstall.

### Pure-Python fallback

Downstream consumers that have a Python implementation of the same logic (e.g. discogs-etl's `scripts/import_csv.py` and `scripts/verify_cache.py`) can force the slower Python path by setting:

```bash
WXYC_ETL_NO_RUST=1
```

Useful when debugging a Rust-vs-Python parity issue or when the wheel isn't built. The `_HAS_WXYC_ETL and not os.environ.get("WXYC_ETL_NO_RUST")` guard pattern is the convention.

## Testing

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
- `wxyc-etl/tests/integration_modules.rs` — cross-module integration scenarios.
- `wxyc-etl/tests/pg_error_tests.rs` — pipeline error / panic propagation against PostgreSQL. Some tests gate on `TEST_DATABASE_URL`; others (`pipeline_error_tests::test_scanner_panic_does_not_deadlock_writer`) are flaky on local macOS but pass in CI.
- `wxyc-etl/tests/panic_recovery.rs` — verifies the pipeline runner doesn't deadlock when a worker panics.
- `wxyc-etl-python/tests/test_performance.py` — marked `@perf`, requires release-built wheel.

Test fixtures use canonical WXYC artist names from the org-level convention. Diacritic-bearing inputs are drawn from `wxycCanonicalArtistNames` in `@wxyc/shared` (Nilüfer Yanya for ü, Csillagrablók for ó, Hermanos Gutiérrez for é). Non-canonical inputs (`10,000 Maniacs`, `Andy Human and the Reptoids`, `Łona`, CJK strings) are kept only where they exercise specific algorithm guards or Unicode behaviors with no canonical analogue.

## CI (`.github/workflows/ci.yml`)

Four jobs on push to `main` and on PR:

| Job | What |
|---|---|
| `lint` | `cargo fmt --check` + `cargo clippy --workspace --all-targets -- -D warnings` (with a small allow-list for established patterns; tighten over time). |
| `test` | `cargo test --workspace`. PG-gated tests skip without `TEST_DATABASE_URL`. |
| `test-postgres` | `cargo test --workspace -- --test-threads=1` against a Postgres 16 service container on port 5433. |
| `python-wheel` | maturin builds a release wheel, pip installs it, runs `pytest wxyc-etl-python/tests/`, then runs the wheel-lifecycle smoke test. |

## Reusable: CI marker / workflow sync check

`scripts/check_marker_ci_sync.py` verifies a repo's pytest marker scheme stays in sync with its CI invocations. It catches the WXYC/discogs-etl#103 failure mode (markers excluded by addopts, no CI job re-selects them, marked tests silently dropped). Sister WXYC repos invoke it via the reusable workflow `.github/workflows/check-ci-marker-sync.yml`:

```yaml
# in another repo's .github/workflows/ci.yml
jobs:
  marker-sync:
    uses: WXYC/wxyc-etl/.github/workflows/check-ci-marker-sync.yml@main
    with:
      repo-path: "."           # or "subpkg/" if pyproject lives in a subdir
      workflows-dir: ".github/workflows"
      tests-dir: "tests"
```

Intentional opt-out for a marker that is, by design, manual-only: add `# ci-sync-skip: <marker> reason: <text>` anywhere in pyproject.toml (the script greps the raw text). See `wxyc-etl-python/pyproject.toml` for a live example (the `perf` marker).

## Consumers

Repos that depend on wxyc-etl by Cargo path or via the Python wheel:

- **discogs-xml-converter** (Rust) — `pipeline`, `text`, `csv_writer`
- **musicbrainz-cache** (Rust) — `pipeline`, `text`, `pg`, `schema`, `state`
- **wikidata-json-filter** (Rust) — `pipeline`, `csv_writer`
- **discogs-etl** (Python via PyO3) — `text`, `state`, `import_utils`, `parser`, `schema`
- **semantic-index** (Python via PyO3) — `text`, `fuzzy`, `parser`, `schema`

Any change that alters a public signature in a module these consumers use should be coordinated with the consumer repo (a follow-up PR), since this crate isn't versioned to crates.io / PyPI — they all pin to the path/source.

## Conventions

- Single-line paragraphs in commit messages and Markdown (org-wide rule).
- TDD when adding new behavior: failing test first, then the implementation.
- When a Python-side parity test changes (`tests/python_parity.rs`), the Python expectation in the consumer repo's tests should change in lockstep.
- Use canonical WXYC artist names in test fixtures (see org-level `CLAUDE.md`); avoid Radiohead, The Beatles, Björk, etc.
