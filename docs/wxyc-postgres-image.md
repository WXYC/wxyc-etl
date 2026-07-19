# wxyc-postgres Docker image

Operator runbook for the `ghcr.io/wxyc/wxyc-postgres` image: a thin overlay over Railway's `ghcr.io/railwayapp-templates/postgres-ssl:N` base that bakes the WXYC `wxyc_unaccent.rules` text-search dictionary into `$SHAREDIR/tsearch_data/`. Built and published from this repo on every `v*.*.*` tag.

The image exists because Postgres looks up `RULES = 'wxyc_unaccent'` against the destination PG's filesystem at `$SHAREDIR/tsearch_data/wxyc_unaccent.rules`, and stock PG images can't self-provision that file at migration time (`/usr/local/share/postgresql/tsearch_data/` is `root:root 0755`, postgres user uid 70 gets EACCES on any write). See WXYC/Backend-Service#805 for the parallel RDS-side outcome that drove WXYC to the Python-analog there; this image is the Railway-side equivalent for the three caches.

## What's in the image

- Everything from `ghcr.io/railwayapp-templates/postgres-ssl:N` — Postgres N, the `init-ssl.sh` initdb hook, `pgbackrest`, `pgvector`, the `wrapper.sh` entrypoint Railway services rely on.
- `data/wxyc_unaccent.rules` from this repo, copied to `/usr/share/postgresql/N/tsearch_data/wxyc_unaccent.rules` (`$SHAREDIR/tsearch_data/`).
- `data/wxyc_unaccent.version` from this repo, copied alongside for version introspection.
- `infra/wxyc-postgres/wxyc-entrypoint.sh`, copied to `/usr/local/bin/` and set as `ENTRYPOINT`. It `exec`s the base `/usr/local/bin/wrapper.sh` unchanged, appending the optional `$WXYC_PG_EXTRA_ARGS` flag list (see [Per-service tuning](#per-service-tuning-wxyc_pg_extra_args)). Each Dockerfile re-declares its own base's `CMD` verbatim because overriding `ENTRYPOINT` resets an inherited `CMD` to empty — and the two bases differ: pg17's is `postgres -p 5432 -c listen_addresses=*` (TCP bind set on the command line), pg16's is `postgres --port=5432` (pg16 binds via `listen_addresses = '*'` in `postgresql.conf`). Both are verified against the pinned digests with `docker buildx imagetools inspect` and guarded by the smoke test's per-version argv assertion.

The image is a **near-pure overlay**. When `WXYC_PG_EXTRA_ARGS` is unset (the default), pulling `:pgN` instead of `ghcr.io/railwayapp-templates/postgres-ssl:N` produces a runtime-identical Postgres instance — the entrypoint `exec`s the base `wrapper.sh` with the base's exact argv, so the same process (PID 1 stays `wrapper.sh`), privilege drop, and SSL provisioning result, plus three extra files on disk. The only runtime difference appears when a service opts in by setting `WXYC_PG_EXTRA_ARGS`, and the effect is confined to that one service.

## Tags

| Tag | Meaning |
|---|---|
| `:pg17` | Latest wxyc-etl release on Postgres 17. Moves on each release. |
| `:pg16` | Latest wxyc-etl release on Postgres 16. Moves on each release. |
| `:pg17-vMAJOR.MINOR.PATCH` | Pinned to a specific wxyc-etl release (e.g. `:pg17-v0.4.1`). Preferred for reproducible CI and rollback targets. |
| `:pg16-vMAJOR.MINOR.PATCH` | Pinned to a specific wxyc-etl release (e.g. `:pg16-v0.4.1`). Same rules. |

Both arches (linux/amd64 + linux/arm64) ship under every tag.

## Railway PG service swap (operator procedure)

The Railway PG plugin lets you swap the underlying image without touching the data volume. Per-cache, one-time operation:

1. Open the Railway dashboard → the cache's project (`discogs-cache`, `musicbrainz-cache`, or `wikidata-cache`).
2. Click the **Postgres** service → **Settings** → **Source**.
3. Change **Image** from `ghcr.io/railwayapp-templates/postgres-ssl:N` to `ghcr.io/wxyc/wxyc-postgres:pgN-vX.Y.Z` (or `:pgN` for the floating tag — pin to a versioned tag in production, floating is fine for staging).
4. Click **Deploy** at the bottom of the panel. Railway will stop the old container and start a new one against the same volume. Downtime is typically 30-60s.
5. Once the new deploy is green, verify the dictionary is live:

```sql
-- From the cache's `Apply pending alembic migrations` runner,
-- or via Railway's "Connect" → SQL panel:
SELECT ts_lexize('wxyc_unaccent', 'café');
-- Expected: {cafe}
```

If you see `{cafe}`, the rules file is on disk and the migration's `CREATE TEXT SEARCH DICTIONARY` step will succeed against this PG.

6. (Optional, for the paranoid) confirm the file landed. The PG version is in the path — `16` for `:pg16`, `17` for `:pg17`:

```sql
SELECT pg_stat_file('/usr/share/postgresql/17/tsearch_data/wxyc_unaccent.rules');
-- Expected: (size, atime, mtime, ctime, false) tuple — not an error.
```

## Per-service tuning (`WXYC_PG_EXTRA_ARGS`)

The three caches share one image but want different Postgres settings (discogs-cache is much larger than musicbrainz-cache / wikidata-cache). To tune one service without touching the others, set the `WXYC_PG_EXTRA_ARGS` environment variable on that Railway service. Its value is appended as extra `postgres -c` flags **from inside the entrypoint**, after the base defaults:

```
WXYC_PG_EXTRA_ARGS=-c shared_buffers=2GB -c effective_cache_size=6GB -c work_mem=16MB -c maintenance_work_mem=512MB -c random_page_cost=1.1 -c effective_io_concurrency=200
```

Set it under the Railway service's **Variables** tab, then redeploy. Verify each setting landed and is sourced from the command line:

```sql
SHOW shared_buffers;                                             -- e.g. 2GB
SELECT name, setting, source FROM pg_settings
 WHERE name IN ('shared_buffers','effective_cache_size','work_mem',
                'maintenance_work_mem','random_page_cost','effective_io_concurrency');
-- source should read 'command line' for each tuned key.
```

Properties:

- **Durable across a volume reprovision.** Railway environment variables survive volume recreation and fresh service re-creation. This is the key advantage over `ALTER SYSTEM` (`postgresql.auto.conf`), which is wiped by a reprovision and silently reverts to stock with no alert.
- **Per-service.** Each cache sets its own value; the shared image carries no cache-specific sizing, so the overlay stays pure for services that don't opt in.
- **Last-wins.** PostgreSQL command-line parsing is last-wins, so an appended `-c shared_buffers=2GB` overrides the inherited default of the same key. It also outranks `postgresql.auto.conf`, so a value set both ways resolves to the command-line one — which makes an `ALTER SYSTEM` → env migration unambiguous (see below).

### Why not a Railway "Custom Start Command"?

A Custom Start Command is the obvious per-service lever, but it **replaces the container entrypoint**, bypassing the base `wrapper.sh` → `docker-entrypoint.sh` chain that drops `root` → the `postgres` user (via `gosu`, keyed on `argv[0] == "postgres"`) and provisions SSL. Applying one to this image crash-loops the service with `"root" execution of the PostgreSQL server is not permitted` (verified live 2026-07-18; see [WXYC/discogs-etl#314](https://github.com/WXYC/discogs-etl/issues/314)). `WXYC_PG_EXTRA_ARGS` runs *inside* the entrypoint instead of replacing it, so the privilege drop, SSL, and the base's TCP binding (via `-c listen_addresses=*` on pg17, or `postgresql.conf` on pg16) all still fire.

### Safety: bounded flag injection, not shell injection

The value is spliced into the postgres invocation via `exec /usr/local/bin/wrapper.sh "$@" ${WXYC_PG_EXTRA_ARGS}` — word-split into additional `postgres` argv entries, **never** through `sh -c "… $VAR"`. So the worst a malformed value can do is inject bounded postgres flags, not shell commands. Setting the variable already requires Railway service access, so this adds no new attack surface. **Do not** "harden" the entrypoint into a shell-form (`bash -c "… $WXYC_PG_EXTRA_ARGS"`) — that reintroduces command injection *and* breaks the privilege drop (the base keys it on `argv[0] == "postgres"`).

One constraint follows from the word-split: because splitting does not honor quotes inside the value, **each flag and its value must be space-free** — `-c shared_buffers=2GB` is fine, but a value with an embedded space like `-c log_line_prefix=%m [%p]` would be split mid-value and hand postgres a stray positional argument, crash-looping startup. Keep to simple `-c key=value` flags with space-free values. (The entrypoint also runs under `set -f`, so glob metacharacters in the value are passed through literally rather than expanded against the working directory.)

### Migrating discogs-cache off `ALTER SYSTEM`

discogs-cache currently pins its [#313](https://github.com/WXYC/discogs-etl/issues/313) tuning via `ALTER SYSTEM` (`postgresql.auto.conf`), which does not survive a reprovision. To make it durable, migrate to `WXYC_PG_EXTRA_ARGS` in this order — because command-line `-c` outranks `postgresql.auto.conf`, both being set at once is safe and the env value wins:

1. Set `WXYC_PG_EXTRA_ARGS` on the discogs-cache service and redeploy the new image. Verify each of the six settings via `SHOW …` and confirm `source = command line`.
2. Only after that verifies, `ALTER SYSTEM RESET` the six values (or `ALTER SYSTEM RESET ALL`) to clear the now-redundant `postgresql.auto.conf` entries.
3. `SELECT pg_reload_conf();` and re-verify the six `SHOW` values are unchanged (now sourced solely from the command line).

### Validate off-prod first

Getting this wrong risks a live database outage (see #314). Validate on a throwaway container or Railway service before pointing a production service at it. Locally: `docker build -f infra/wxyc-postgres/Dockerfile.pgN .`, then `bash infra/wxyc-postgres/smoke-test.sh <image> N` — the smoke suite asserts the flags apply, last-wins over the default, run as the `postgres` user, and SSL stays on, both with and without the env set.

> Note on shutdown signals: the pinned base's `wrapper.sh` runs `docker-entrypoint.sh` without `exec` and installs no signal traps, so `docker stop` / a Railway redeploy does not perform a clean postgres shutdown (the next boot does a trivial "redo is not required" crash recovery). This is a property of the base image, unchanged by this overlay (`wxyc-entrypoint.sh` `exec`s `wrapper.sh`, so PID 1 stays `wrapper.sh` and no extra parent is inserted). It is called out here only so operators aren't surprised by the recovery line in the logs.

## Rollback

Same procedure in reverse:

1. Railway → service → **Settings** → **Source** → set **Image** back to `ghcr.io/railwayapp-templates/postgres-ssl:N`.
2. **Deploy**.
3. Migrations that reference `wxyc_unaccent` will fail with the now-actionable `F0000` error pointing back at this runbook. That's expected; rollback means dictionary-dependent migrations are off the menu until you re-swap.

Data is unaffected. Existing rows already normalized via `wxyc_unaccent` stay normalized — only future calls to `ts_lexize('wxyc_unaccent', ...)` and `SELECT wxyc_unaccent(...)`-style references would fail.

## Refreshing the base image (security updates)

When `ghcr.io/railwayapp-templates/postgres-ssl:N` ships a security update:

1. Look up the new digest:

```bash
gh api /orgs/railwayapp-templates/packages/container/postgres-ssl/versions \
  --jq '.[] | select(.metadata.container.tags[] | startswith("17") or startswith("16")) | {tags: .metadata.container.tags, name: .name}'
```

2. Update the digest in both `infra/wxyc-postgres/Dockerfile.pg17` and `Dockerfile.pg16`.
3. Bump the workspace version in `Cargo.toml` (patch-level — additive, no API change).
4. `git tag v0.X.Y && git push origin v0.X.Y` — `release.yml` rebuilds and pushes the image to GHCR.
5. Swap each cache's Railway PG service to the new versioned tag.

## Refreshing the rules file

Editing `data/wxyc_unaccent.rules` is owned by the generator at `wxyc-etl/tests/wxyc_unaccent_rules_test.rs`. Run with `WXYC_REGENERATE_RULES=1` after any `to_match_form` behavior change in the Rust crate. The `wxyc_unaccent.version` file is bumped by the same generator. Once committed:

1. Bump `Cargo.toml` workspace version.
2. Tag and push — `release.yml` picks up the new rules in the next image build.
3. Swap each cache's Railway PG service to the new versioned tag. Re-running the cache's migration is a no-op (the `CREATE TEXT SEARCH DICTIONARY` step is `DROP IF EXISTS + CREATE`).

## Why a custom image vs. self-provisioning at migration time

Tried, infeasible. Verified 2026-05-20 against `postgres:16-alpine` (`/usr/local/share/postgresql/tsearch_data/`) and `ghcr.io/railwayapp-templates/postgres-ssl:17` (`/usr/share/postgresql/17/tsearch_data/`): the `postgres` OS user (uid 70) cannot write to `$SHAREDIR/tsearch_data/` (`root:root 0755`) regardless of which image layout the destination uses, so no `lo_export` / `COPY ... TO PROGRAM` / equivalent issued from inside a migration can land the file. The Backend-Service team hit the same constraint against AWS RDS and pivoted to the Python-analog path (WXYC/Backend-Service#805); the three caches stay on the Postgres-analog path by virtue of Railway exposing the image as a swappable property.

## Visibility

The image is **public** on GHCR. Bytes are the canonical rules file (already public in this repo's `data/`) plus the stock railwayapp-templates base; nothing sensitive ships in this layer.
