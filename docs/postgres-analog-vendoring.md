## Postgres analog: vendoring + SHA-256 pin

The cross-cache-identity match form ships in two places:

- **Rust + Python** (this repo) — `wxyc_etl::text::to_identity_match_form` and its three siblings.
- **Postgres** (each consumer repo) — a deploy of `wxyc_identity_match_artist` and its three siblings, applied as plpgsql functions in the consumer's migration tooling.

The Postgres half is parametric on two files vendored verbatim from this repo:

- `data/wxyc_unaccent.rules` — Latin / Greek mark-strip + fold rules consumed by the `unaccent` extension. Generated from `to_match_form` over the same codepoint scope `strip_combining_selective` covers. Asserted byte-for-byte by `tests/wxyc_unaccent_rules_test.rs`.
- `data/wxyc_identity_match_functions.sql` — the four canonical plpgsql functions plus shared helpers (`wxyc_match_form`, `wxyc_strip_trailing_parens`, `wxyc_drop_articles`, `wxyc_identity_baseline`).

A third metadata file is part of the contract:

- `data/wxyc_unaccent.version` — single-line semver string. Bumped on every behavior-affecting edit to the rules file. The version is the human-readable handle consumers cite in their migration's version assertion.

The rules file is **not commentable**: Postgres `unaccent` parses every non-empty line as a rule (`#` is not a comment marker, despite what some Postgres docs imply). The version header has to live outside.

### Pin file (consumer side)

Each consumer cache repo carries a `wxyc-etl-pin.txt` at its root, recording the exact source-of-truth bytes it vendored. Format:

```
# wxyc-etl postgres-analog pin
# Bump this file in lockstep with any data/wxyc_unaccent.rules update.

unaccent_rules_version = 0.1.0
unaccent_rules_sha256  = <64-hex>
functions_sql_sha256   = <64-hex>
wxyc_etl_version       = 0.4.0
```

Compute the SHAs over the `data/` files in the wxyc-etl release tag you vendored from:

```sh
sha256sum data/wxyc_unaccent.rules data/wxyc_identity_match_functions.sql
```

### Consumer CI: pin verification

Each consumer adds a CI step that fails when the local checkout's vendored files diverge from the pinned SHAs. A reference shell snippet:

```sh
PIN=$(grep '^unaccent_rules_sha256' wxyc-etl-pin.txt | awk '{print $3}')
GOT=$(sha256sum <path/to/vendored/wxyc_unaccent.rules> | awk '{print $1}')
[ "$PIN" = "$GOT" ] || { echo "rules file drift; re-vendor from wxyc-etl@$VERSION"; exit 1; }
```

Same shape for the SQL file.

### Migration version assertion (consumer side)

The authoritative version check is the SHA-256 pin in `wxyc-etl-pin.txt`, verified by CI before the migration runs. The consumer's deploy tooling is responsible for ensuring the file landed at `$SHAREDIR/tsearch_data/wxyc_unaccent.rules` and matches the pinned SHA.

A runtime check inside the migration is **optional** and constrained: `pg_read_file` resolves relative paths against the cluster's data directory (`$PGDATA`), not `$SHAREDIR`, so there is no portable relative path to the rules file. Consumers that want a defense-in-depth runtime check have two options:

1. **Hardcode an absolute path** (server-install-specific; requires the calling role to have `pg_read_server_files` membership):

   ```sql
   DO $$
   DECLARE
     deployed text;
     expected constant text := '0.1.0';  -- update in lockstep with wxyc-etl-pin.txt
     rules_path constant text :=
       '/usr/local/share/postgresql/tsearch_data/wxyc_unaccent.version';  -- alpine
   BEGIN
     SELECT trim(pg_read_file(rules_path, 0, 64, true)) INTO deployed;
     IF deployed IS NULL OR deployed != expected THEN
       RAISE EXCEPTION 'wxyc_unaccent version mismatch: deployed=%, expected=%',
         deployed, expected;
     END IF;
   END $$;
   ```

   This requires the consumer's migration to know its server install layout (alpine vs Debian vs Homebrew differ; see `scripts/install_wxyc_unaccent.sh`).

2. **Skip the runtime check entirely** and rely on the SHA pin + CI verification step described in the previous section. Recommended default for new consumers — the SHA is a stronger guarantee than a version string anyway.

### Installing the rules file on the server

The Postgres `unaccent` extension loads rules files from `$SHAREDIR/tsearch_data/`. The local-dev helper `scripts/install_wxyc_unaccent.sh` copies both files into the right place using `pg_config --sharedir`:

```sh
bash scripts/install_wxyc_unaccent.sh
```

CI installs them with `docker cp` into the postgres service container before the parity test runs (see `.github/workflows/ci.yml::test-postgres`).

Consumer repo migration tooling is expected to do the equivalent on its deploy target — either by shelling out at migration time or by pre-baking the files into the server image.

### Bump procedure

1. Edit `wxyc_etl::text` Rust code such that `to_match_form` changes its behavior on some codepoint.
2. Regenerate: `WXYC_REGENERATE_RULES=1 cargo test --test wxyc_unaccent_rules_test`. This rewrites both `data/wxyc_unaccent.rules` and `data/wxyc_unaccent.version` based on the new behavior. Bump the version constant in `tests/wxyc_unaccent_rules_test.rs` if the rules diff is behavior-affecting.
3. Tag a new wxyc-etl release (workspace version bump → tag push).
4. Each consumer opens a vendor-bump PR:
   - Replace its vendored copies of `wxyc_unaccent.rules` + `wxyc_unaccent.version` + `wxyc_identity_match_functions.sql`.
   - Update `wxyc-etl-pin.txt` SHAs + versions.
   - Update the `expected` literal in its migration version-assertion DO block (if the consumer enabled the runtime check).
   - Re-run its repo's parity test against the new files.

### Known coverage limits

- **Cf strip is BMP-only.** `wxyc_match_form` strips format characters from the Basic Multilingual Plane (U+0000–U+FFFF) but not from supplementary planes (U+E0001 language tag, U+E0020–U+E007F tag characters, U+13430–U+13438 Egyptian hieroglyph format controls). Rust's `strip_cf_except_zwj` covers the full Cf category. If a future `to_match_form` change adds supplementary-plane Cf to the fixture, the rules file alone won't unblock parity — the plpgsql `cf_pattern` builder also has to grow. The bump procedure should re-run the parity test against the new fixture before declaring rules-only changes sufficient.
- **Mojibake repair is application-side.** `to_storage_form`'s ftfy-style mojibake fix runs in Rust before `to_match_form` sees the input. The Postgres pipeline assumes inputs are already storage-form (post-mojibake). Catalog writers are responsible for that pass.

### Why two files instead of one rules file with an embedded header

The Postgres `unaccent` parser treats every non-empty line as a rule and emits a warning ("invalid syntax: more than two strings in unaccent rule") on anything that looks like a comment. Splitting the version out of the rules file keeps the canonical file warning-free while still pinning a human-readable version handle in the deploy.
