# wxyc-etl

Shared Rust crate (with PyO3 Python bindings) that supplies the cross-cutting primitives used by every WXYC ETL repo: text normalization, fuzzy matching, PostgreSQL bulk loading, a parallel pipeline framework, a MySQL-dump parser, schema constants, and JSON state tracking.

## Tag Stability Policy (READ BEFORE EDITING `.github/workflows/`)

This repo publishes a reusable GitHub Actions workflow that other WXYC repos consume by tag:

- `check-ci-marker-sync.yml` — consumed as `WXYC/wxyc-etl/.github/workflows/check-ci-marker-sync.yml@gha/v1`

`gha/v1` is a **moving major tag**. It points at the latest commit on `main` that is non-breaking for the v1 contract. Consumers pin to `@gha/v1` to opt into compatible improvements; they pin to a SHA only if they want frozen behavior.

### Before changing any reusable workflow, decide: is this breaking?

A change is **breaking** if it does any of the following to a `workflow_call`-enabled file:

1. Adds a new required `inputs:` entry, or removes/renames an existing input.
2. Adds a new required `secrets:` entry, or removes/renames an existing secret.
3. Removes or renames an `outputs:` entry.
4. Changes the default value of an existing input in a way a consumer could depend on.
5. Changes observable behavior consumers rely on — e.g. the marker-sync check now rejects a marker scheme it previously accepted, the runner OS major version bumps, a step that produced an artifact stops producing it.

Anything else is **non-breaking**: bugfixes, perf work, internal refactors, *additive* optional inputs/outputs/secrets, dependency bumps that don't change observable behavior. Note: changes to `scripts/check_marker_ci_sync.py` flow into consumers via this workflow — apply the same checklist to that script.

### The bump procedure

**Non-breaking change** — re-point `gha/v1` at the new commit after merge:

```bash
git fetch origin
git tag -f gha/v1 origin/main
git push --force origin gha/v1
```

**Breaking change** — *do not move `gha/v1`*. Cut `gha/v2` instead:

```bash
git tag -a gha/v2 -m "v2: <one-line summary of what broke>" origin/main
git push origin gha/v2
```

Then file a migration ticket in every consumer repo that pins `@gha/v1` for this workflow. Search the org with `gh search code 'WXYC/wxyc-etl/.github/workflows/check-ci-marker-sync.yml@gha/v1'` to find them.

### Why this matters

Force-pushing `gha/v1` past a breaking change silently breaks every consumer's CI the next time their workflow fires. Consumers have no signal — the `@gha/v1` ref is the same string they had yesterday. The cost of cutting `gha/v2` is one tag and one round of consumer PRs; the cost of breaking `gha/v1` is debugging in a dozen repos at once.

### Caller permissions contract

Callers of `check-ci-marker-sync.yml` must grant at minimum:

```yaml
permissions:
  contents: read   # the reusable workflow checks out the calling repo
```

The workflow declares no `permissions:` block of its own — it doesn't write anywhere, just runs `scripts/check_marker_ci_sync.py` against the caller's checkout. The caller's `contents: read` is enough; granting less makes the `actions/checkout` step fail.

**Escalating the required caller permissions is itself a breaking change** (rule 5 above — observable behavior). If a revision of this workflow needs another scope from the caller (e.g., `pull-requests: write` to comment on a PR with the marker diff), cut `gha/v2` and migrate consumers. The asymmetry matters: dropping a required scope is non-breaking; adding one breaks every caller that hardened to the previous floor.

Watch for the **caller-callee narrowing trap** when adding a `permissions:` block to this workflow: if a reusable workflow declares `contents: write` at the workflow level (e.g., to push a fix-up commit) but its callers hardened to `contents: read`, the matrix run startup_failures with no jobs and no obvious error. See [WXYC/Backend-Service#857](https://github.com/WXYC/Backend-Service/issues/857) (silent for 10 commits across 2 days) and PR [#858](https://github.com/WXYC/Backend-Service/pull/858) for the recovery pattern. `check-ci-marker-sync.yml` is read-only today, so it can't trip this — but the trap applies to any future revision that takes a write scope, and a `gha/v2` migration is the safest way to surface it.

### Docker image consumers (`ghcr.io/wxyc/wxyc-postgres`)

This repo also publishes the `wxyc-postgres` image (see `infra/wxyc-postgres/` and `docs/wxyc-postgres-image.md`), consumed by Railway PG services in discogs-cache, musicbrainz-cache, and wikidata-cache. Image tags are a separate consumer contract:

| Tag | Consumer expectation |
|---|---|
| `:pg17`, `:pg16` | Floating. Moves on every wxyc-etl release. Suitable for staging. |
| `:pg17-vMAJOR.MINOR.PATCH`, `:pg16-vMAJOR.MINOR.PATCH` | Pinned (e.g. `:pg17-v0.4.1`). Production Railway services pin here; rollback target. |

The base image must stay on `ghcr.io/railwayapp-templates/postgres-ssl:N@sha256:<digest>` (digest-pinned, not floating). Refreshing the base digest is a release event — bump `Cargo.toml`, tag, ship a new pinned image, then operators swap each Railway PG one-by-one. **Don't move the base off `railwayapp-templates/postgres-ssl`** — Railway services depend on its SSL init hook, pgbackrest, pgvector, and `wrapper.sh` entrypoint; switching to stock `postgres:N` would silently strip all four.

A behavior change in `data/wxyc_unaccent.rules` (the bytes consumers see at `$SHAREDIR/tsearch_data/wxyc_unaccent.rules` after swap) is breaking. Today the rules file is the canonical Postgres-side counterpart to `strip_combining_selective + apply_folds`, locked by `wxyc-etl/tests/wxyc_unaccent_rules_test.rs` against the Rust generator; any change to `to_match_form` flows through that test to a new bytes-on-disk for every consumer. Coordinate with the three cache repos before merging such a change.

## Workspace Layout

Cargo workspace with two members:

| Crate | Path | Purpose |
|---|---|---|
| `wxyc-etl` | `wxyc-etl/` | Pure Rust library. Used by other Rust ETL repos (discogs-xml-converter, musicbrainz-cache, wikidata-cache) via `[dependencies] wxyc-etl = { path = "../wxyc-etl" }`. |
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
- `wxyc-etl/tests/identity_normalization_parity.rs` — locks the cross-cache-identity layered normalizer (`to_identity_match_form` and the opt-in step-6/step-8 variants) against `wxyc-etl/tests/fixtures/identity_normalization_cases.csv`. The CSV is the spec — Postgres analog `wxyc_identity_match_artist` (when it ships) targets the same outputs byte-for-byte. Updating the CSV requires re-deriving from `docs/normalization.md`, not from the Rust output.
- `wxyc-etl/tests/wxyc_unaccent_rules_test.rs` — generator + freshness check for `data/wxyc_unaccent.rules` (the Postgres-side counterpart to `strip_combining_selective` + `apply_folds`). Default mode asserts the committed rules + version files match what the generator would emit from `to_match_form`. Run with `WXYC_REGENERATE_RULES=1` to refresh after any `to_match_form` behavior change.
- `wxyc-etl/tests/postgres_parity_test.rs` — `#[ignore]`-gated Rust ↔ Postgres byte-equality check for the four identity-match entry points. Driven by `tests/fixtures/identity_normalization_cases.csv`. Local setup: `bash scripts/install_wxyc_unaccent.sh` to copy `data/wxyc_unaccent.rules` into `$SHAREDIR/tsearch_data/`; then `TEST_DATABASE_URL=... cargo test --test postgres_parity_test -- --include-ignored`. CI installs the rules with `docker cp` into the service container.
- `wxyc-etl/tests/regression_report.rs` — generates `target/regression-report.json` per plan §3.3.4. Run with `WXYC_REGRESSION_INPUT=/path/to/artists.csv cargo test --test regression_report -- --include-ignored`. Without the env var the harness still runs a self-test against the parity matrix to keep the JSON schema honest. Sections A + B (per-row diff, per-step impact) come from the input CSV alone; sections C + D (match-shift, per-step shift) are computed from in-memory partition analysis; section E (confidence threshold validation) is a stub — it requires joining `entity.identity.confidence` from a live Homebrew cache and is left for a follow-up PR or for an inline run.
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
    uses: WXYC/wxyc-etl/.github/workflows/check-ci-marker-sync.yml@gha/v1
    with:
      repo-path: "."           # or "subpkg/" if pyproject lives in a subdir
      workflows-dir: ".github/workflows"
      tests-dir: "tests"
```

Intentional opt-out for a marker that is, by design, manual-only: add `# ci-sync-skip: <marker> reason: <text>` anywhere in pyproject.toml (the script greps the raw text). See `wxyc-etl-python/pyproject.toml` for a live example (the `perf` marker).

## Releases

Three artifacts publish from the same tag — `wxyc-etl` to crates.io, `wxyc-etl-python` to PyPI, and `ghcr.io/wxyc/wxyc-postgres:{pg17,pg16}` to GHCR. The workspace `version` in the root `Cargo.toml` is the single source of truth; both crates inherit it via `version.workspace = true`, maturin reads it via `pyproject.toml`'s `dynamic = ["version"]`, and the Docker image tags it as `:pgN-vX.Y.Z` alongside the floating `:pgN`.

### Cutting a release

```bash
# 1. Bump the workspace version
$EDITOR Cargo.toml          # change [workspace.package] version
git commit -am "chore: bump workspace version to 0.X.Y"

# 2. Tag and push
git tag v0.X.Y
git push origin main --tags
```

The `release.yml` workflow fires on tags matching `v*.*.*`. It:
1. Verifies the tag matches `Cargo.toml`'s workspace version.
2. `cargo publish -p wxyc-etl` to crates.io.
3. Builds wheels for `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`, `x86_64-apple-darwin`, `aarch64-apple-darwin` plus an sdist.
4. `maturin upload --skip-existing dist/*` to PyPI.
5. Builds + smoke-tests `ghcr.io/wxyc/wxyc-postgres:pg17` and `:pg16` (single-arch local build, `pg_stat_file` + `ts_lexize('wxyc_unaccent', 'café')` against the running container, SHA-256 of the rules file inside the image vs. `data/wxyc_unaccent.rules` on disk). Then `docker/build-push-action@v6` pushes both arches (`linux/amd64`, `linux/arm64`) with floating + pinned tags.

The base for the image is digest-pinned in `infra/wxyc-postgres/Dockerfile.pgN`. Refreshing it is a release event — see `docs/wxyc-postgres-image.md` for the procedure.

### Rehearsing a release

Use the `workflow_dispatch` trigger with `dry_run: true` (the default). It runs every build step + a `cargo publish --dry-run` + a local image build with smoke test, and skips the actual upload / push steps. Safe to run any time.

### Required repo secrets

| Secret | Source |
|---|---|
| `CRATES_IO_TOKEN` | https://crates.io/me — `publish-new` + `publish-update` scopes |
| `PYPI_API_TOKEN` | https://pypi.org/manage/account/token/ — scope to the `wxyc-etl` project after the first publish |

### Versioning

Stay in 0.x lockstep until rec 5 (publish) and rec 7 (migrations) both ship; cut 1.0 after that. Hard cuts in 0.x are acceptable; introduce a deprecation cycle post-1.0. See `project_pipeline_hardening` memory for the agreed-upon decisions.

## Consumers

Repos that depend on wxyc-etl by Cargo path or via the Python wheel:

- **discogs-xml-converter** (Rust) — `pipeline`, `text`, `csv_writer`
- **musicbrainz-cache** (Rust) — `pipeline`, `text`, `pg`, `schema`, `state`
- **wikidata-cache** (Rust) — `pipeline`, `csv_writer`
- **discogs-etl** (Python via PyO3) — `text`, `state`, `import_utils`, `parser`, `schema`
- **semantic-index** (Python via PyO3) — `text`, `fuzzy`, `parser`, `schema`

Any change that alters a public signature in a module these consumers use should be coordinated with the consumer repo (a follow-up PR), since this crate isn't versioned to crates.io / PyPI — they all pin to the path/source.

## Scheduling Policy

Every WXYC ETL fits one of three buckets. The bucket determines the scheduler — we don't mix.

| Workload type | Scheduler | Why |
|---|---|---|
| Stateless ETL (cache rebuilds, daily syncs) | GitHub Actions cron | Free, observable, retried by re-running the workflow; no host to maintain |
| Continuous worker / poller | Railway service | Long-running with restart-on-crash; cheap for steady traffic |
| Periodic job tightly coupled to a service's API | In-process scheduler in that service | Avoids network hop and shared deploy unit with the API |

Add `workflow_dispatch:` to any cron-driven workflow so it can be invoked manually for ad-hoc runs without re-tagging or merging.

### ETL audit (as of 2026-04-27)

| ETL | Workload | Today | Target |
|---|---|---|---|
| `discogs-etl` daily library sync | stateless | GH Actions cron | ✓ on policy |
| `discogs-etl` monthly cache rebuild | stateless | manual | GH Actions cron + `workflow_dispatch` (issue #118) |
| `musicbrainz-cache` rebuild | stateless | manual | GH Actions cron + `workflow_dispatch` (issue #26) |
| `wikidata-cache` rebuild | stateless | manual | GH Actions cron + `workflow_dispatch` (issue #17) |
| `semantic-index` nightly sync | API-coupled | in-process scheduler | ✓ on policy |
| `Backend-Service` `jobs/flowsheet-etl/` | continuous worker | EC2 polling | ✓ on policy (Railway-equivalent) |
| `discogs-xml-converter` | invoked by `discogs-etl` | manual / on-demand | no independent schedule needed |
| `semantic-index` AcousticBrainz import | one-shot | manual | no schedule needed |
| `semantic-index` audio archive processing | manual | manual | no schedule needed |

Outliers tracked under epic [wxyc-etl#47](https://github.com/WXYC/wxyc-etl/issues/47).

## Conventions

- Single-line paragraphs in commit messages and Markdown (org-wide rule).
- TDD when adding new behavior: failing test first, then the implementation.
- When a Python-side parity test changes (`tests/python_parity.rs`), the Python expectation in the consumer repo's tests should change in lockstep.
- Use canonical WXYC artist names in test fixtures (see org-level `CLAUDE.md`); avoid Radiohead, The Beatles, Björk, etc.
