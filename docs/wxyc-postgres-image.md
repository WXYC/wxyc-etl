# wxyc-postgres Docker image

Operator runbook for the `ghcr.io/wxyc/wxyc-postgres` image: a thin overlay over Railway's `ghcr.io/railwayapp-templates/postgres-ssl:N` base that bakes the WXYC `wxyc_unaccent.rules` text-search dictionary into `$SHAREDIR/tsearch_data/`. Built and published from this repo on every `v*.*.*` tag.

The image exists because Postgres looks up `RULES = 'wxyc_unaccent'` against the destination PG's filesystem at `$SHAREDIR/tsearch_data/wxyc_unaccent.rules`, and stock PG images can't self-provision that file at migration time (`/usr/local/share/postgresql/tsearch_data/` is `root:root 0755`, postgres user uid 70 gets EACCES on any write). See WXYC/Backend-Service#805 for the parallel RDS-side outcome that drove WXYC to the Python-analog there; this image is the Railway-side equivalent for the three caches.

## What's in the image

- Everything from `ghcr.io/railwayapp-templates/postgres-ssl:N` — Postgres N, the `init-ssl.sh` initdb hook, `pgbackrest`, `pgvector`, the `wrapper.sh` entrypoint Railway services rely on.
- `data/wxyc_unaccent.rules` from this repo, copied to `/usr/share/postgresql/N/tsearch_data/wxyc_unaccent.rules` (`$SHAREDIR/tsearch_data/`).
- `data/wxyc_unaccent.version` from this repo, copied alongside for version introspection.

The image is a pure overlay — no other config or behavior change. Pulling `:pgN` instead of `ghcr.io/railwayapp-templates/postgres-ssl:N` produces a byte-identical Postgres instance with two extra files on disk.

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
