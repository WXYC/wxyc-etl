#!/usr/bin/env bash
#
# Install data/wxyc_unaccent.rules into the local Postgres server's
# tsearch_data directory so the `wxyc_unaccent` text-search dictionary
# can be created. Required by `wxyc-etl/tests/postgres_parity_test.rs`
# and by consumer cache migrations.
#
# Usage:
#   bash scripts/install_wxyc_unaccent.sh                  # default: pg_config in PATH
#   PG_CONFIG=/path/to/pg_config bash scripts/install_wxyc_unaccent.sh
#
# CI: the test-postgres job uses this script after `apt-get install postgresql-server-dev-...`.
set -euo pipefail

cd "$(dirname "$0")/.."

PG_CONFIG="${PG_CONFIG:-pg_config}"
if ! command -v "$PG_CONFIG" >/dev/null 2>&1; then
  echo "error: $PG_CONFIG not found on PATH. Set PG_CONFIG=/path/to/pg_config." >&2
  exit 1
fi

# Prefer the server pg_config (libpq's may point at a client-only install
# without tsearch_data). The user can override via PG_SHAREDIR.
SHAREDIR="${PG_SHAREDIR:-$($PG_CONFIG --sharedir)}"
DEST="$SHAREDIR/tsearch_data"

if [[ ! -d "$DEST" ]]; then
  echo "error: $DEST does not exist; is the server installed?" >&2
  exit 1
fi

SRC="data/wxyc_unaccent.rules"
if [[ ! -f "$SRC" ]]; then
  echo "error: $SRC missing — run the wxyc_unaccent_rules_test to regenerate." >&2
  exit 1
fi

cp -v "$SRC" "$DEST/wxyc_unaccent.rules"
echo "installed to $DEST/wxyc_unaccent.rules"
