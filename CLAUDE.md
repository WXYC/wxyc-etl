# wxyc-etl

Shared Rust crate (with PyO3 Python bindings) that supplies the cross-cutting primitives used by every WXYC ETL repo: text normalization, fuzzy matching, PostgreSQL bulk loading, a parallel pipeline framework, a MySQL-dump parser, schema constants, and JSON state tracking.

## Topic guides

CLAUDE.md is a router for the always-loaded reference card. Topic depth lives in `docs/`:

- **[`docs/architecture.md`](docs/architecture.md)** — Workspace layout (Rust crate + PyO3 bindings), per-module purpose table (`text`, `pg`, `pipeline`, `csv_writer`, `sqlite`, `state`, `import`, `schema`, `fuzzy`, `parser`, `logger`, `cli`)
- **[`docs/observability.md`](docs/observability.md)** — `wxyc_etl::logger::init` (Rust) / `init_logger` (Python), required tag taxonomy (`repo`, `tool`, `step`, `run_id`), Sentry forwarding via `SENTRY_DSN`
- **[`docs/cli-convention.md`](docs/cli-convention.md)** — Cache-builder CLI shape (`build` / `import` subcommands, `--database-url` with env fallback, `#[command(flatten)]` composition rule)
- **[`docs/python-bindings.md`](docs/python-bindings.md)** — `wxyc-etl-python` layout, maturin packaging, exposed vs unexposed submodules, editable install for downstream repos, `WXYC_ETL_NO_RUST=1` pure-Python fallback
- **[`docs/testing.md`](docs/testing.md)** — Test commands, notable test files (Python parity, identity-normalization parity, `wxyc_unaccent.rules` freshness, Rust↔Postgres parity, regression report), fixture convention
- **[`docs/ci.md`](docs/ci.md)** — CI job table, reusable marker-sync workflow consumption pattern, three-bucket scheduling policy + ETL audit
- **[`docs/releases.md`](docs/releases.md)** — Three-artifact release (crates.io + PyPI + `ghcr.io/wxyc/wxyc-postgres`), cutting/rehearsing/secrets, versioning policy

Read the relevant topic doc before doing work in that area.

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

Consumers may set the optional `WXYC_PG_EXTRA_ARGS` env var on a Railway PG service to append per-service `postgres -c` tuning flags (durable across a volume reprovision, unlike `ALTER SYSTEM`). It is honored inside the image's entrypoint, is a no-op when unset (runtime-identical to the base), and is documented in `docs/wxyc-postgres-image.md`. It's a consumer-facing surface: renaming or changing its append semantics is a breaking change for any service that sets it.

The base image must stay on `ghcr.io/railwayapp-templates/postgres-ssl:N@sha256:<digest>` (digest-pinned, not floating). Refreshing the base digest is a release event — bump `Cargo.toml`, tag, ship a new pinned image, then operators swap each Railway PG one-by-one. **Don't move the base off `railwayapp-templates/postgres-ssl`** — Railway services depend on its SSL init hook, pgbackrest, pgvector, and `wrapper.sh` entrypoint; switching to stock `postgres:N` would silently strip all four.

A behavior change in `data/wxyc_unaccent.rules` (the bytes consumers see at `$SHAREDIR/tsearch_data/wxyc_unaccent.rules` after swap) is breaking. Today the rules file is the canonical Postgres-side counterpart to `strip_combining_selective + apply_folds`, locked by `wxyc-etl/tests/wxyc_unaccent_rules_test.rs` against the Rust generator; any change to `to_match_form` flows through that test to a new bytes-on-disk for every consumer. Coordinate with the three cache repos before merging such a change.

## Consumers

Repos that depend on wxyc-etl by Cargo path or via the Python wheel:

- **discogs-xml-converter** (Rust) — `pipeline`, `text`, `csv_writer`
- **musicbrainz-cache** (Rust) — `pipeline`, `text`, `pg`, `schema`, `state`
- **wikidata-cache** (Rust) — `pipeline`, `csv_writer`
- **discogs-etl** (Python via PyO3) — `text`, `state`, `import_utils`, `parser`, `schema`
- **semantic-index** (Python via PyO3) — `text`, `fuzzy`, `parser`, `schema`

Any change that alters a public signature in a module these consumers use should be coordinated with the consumer repo (a follow-up PR), since this crate isn't versioned to crates.io / PyPI — they all pin to the path/source.

## Conventions

- Single-line paragraphs in commit messages and Markdown (org-wide rule).
- TDD when adding new behavior: failing test first, then the implementation.
- When a Python-side parity test changes (`tests/python_parity.rs`), the Python expectation in the consumer repo's tests should change in lockstep.
- Use canonical WXYC artist names in test fixtures (see org-level `CLAUDE.md`); avoid Radiohead, The Beatles, Björk, etc.
