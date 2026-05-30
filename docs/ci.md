# CI

## Jobs (`.github/workflows/ci.yml`)

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
