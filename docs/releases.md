# Releases

Three artifacts publish from the same tag — `wxyc-etl` to crates.io, `wxyc-etl-python` to PyPI, and `ghcr.io/wxyc/wxyc-postgres:{pg17,pg16}` to GHCR. The workspace `version` in the root `Cargo.toml` is the single source of truth; both crates inherit it via `version.workspace = true`, maturin reads it via `pyproject.toml`'s `dynamic = ["version"]`, and the Docker image tags it as `:pgN-vX.Y.Z` alongside the floating `:pgN`.

## Cutting a release

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

## Rehearsing a release

Use the `workflow_dispatch` trigger with `dry_run: true` (the default). It runs every build step + a `cargo publish --dry-run` + a local image build with smoke test, and skips the actual upload / push steps. Safe to run any time.

## Required repo secrets

| Secret | Source |
|---|---|
| `CRATES_IO_TOKEN` | https://crates.io/me — `publish-new` + `publish-update` scopes |
| `PYPI_API_TOKEN` | https://pypi.org/manage/account/token/ — scope to the `wxyc-etl` project after the first publish |

## Versioning

Stay in 0.x lockstep until rec 5 (publish) and rec 7 (migrations) both ship; cut 1.0 after that. Hard cuts in 0.x are acceptable; introduce a deprecation cycle post-1.0. See `project_pipeline_hardening` memory for the agreed-upon decisions.
