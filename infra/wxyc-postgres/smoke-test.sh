#!/usr/bin/env bash
#
# smoke-test.sh <image> <pg_major> — end-to-end smoke test for the
# ghcr.io/wxyc/wxyc-postgres image. Runs in CI (release.yml, per-arch matrix)
# and is runnable locally against a `docker build`-ed image for off-prod
# validation evidence.
#
# It asserts, in order:
#   1. Baked dictionary integrity      — wxyc_unaccent.rules/.version SHA + ts_lexize.
#   2. Runtime-identical baseline (unset) — runs as the `postgres` user, SSL on,
#      listen_addresses='*', no "root" execution error.
#   3. WXYC_PG_EXTRA_ARGS feature      — the 6-flag string applies as six distinct
#      settings, last-wins over the stock default, sourced from the command line,
#      and the privilege drop + SSL still hold.
#   4. Process-model + redeploy integrity — PID 1 stays the base wrapper.sh (our
#      wxyc-entrypoint.sh `exec`s it, inserting no extra parent), the postmaster
#      receives exactly the composed argv (its base's CMD + appended extra), and
#      a full container replacement keeps the tuning applied. NOTE: the pinned base's
#      wrapper.sh runs docker-entrypoint.sh WITHOUT `exec` and installs no signal
#      traps, so it does not forward SIGINT/SIGTERM to the postmaster; clean
#      shutdown on signal is a base-image concern, unchanged by this overlay and
#      out of scope (see the section-4 comment).
#
# Requires: docker, and read access to <repo>/data (resolved relative to this
# script, not $CWD).
set -euo pipefail

IMG="${1:?usage: smoke-test.sh <image> <pg_major>}"
PG="${2:?usage: smoke-test.sh <image> <pg_major>}"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
DATA_DIR="${REPO_ROOT}/data"
SHAREDIR="/usr/share/postgresql/${PG}"

# The full per-service tuning example from WXYC/wxyc-etl#145 "Desired end state".
# `shared_buffers=2GB` deliberately duplicates the stock 128MB default to prove
# last-wins precedence; the two SSD flags close the multi-flag word-split check.
EXTRA_ARGS_FULL="-c shared_buffers=2GB -c effective_cache_size=6GB -c work_mem=16MB -c maintenance_work_mem=512MB -c random_page_cost=1.1 -c effective_io_concurrency=200"

C_BASE="wxyc-pg-smoke-base"
C_ENV="wxyc-pg-smoke-env"
C_SIG="wxyc-pg-smoke-sig"
VOL_SIG="wxyc-pg-smoke-sigvol"

# ---------------------------------------------------------------------------
# helpers
# ---------------------------------------------------------------------------
sha256_of() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{print $1}'
  else
    shasum -a 256 "$1" | awk '{print $1}'
  fi
}

fail() { echo "::error ::$*" >&2; exit 1; }

assert_eq() { # <desc> <expected> <actual>
  if [[ "$3" != "$2" ]]; then
    fail "FAIL: $1 — expected [$2], got [$3]"
  fi
  echo "PASS: $1 = $3"
}

assert_no_root_error() { # <container> <label> — the privilege drop must have fired
  # Capture logs into a variable first, THEN grep. Piping `docker logs` straight
  # into `grep -q` is unsafe under `set -o pipefail`: grep -q exits on first match
  # and can SIGPIPE `docker logs` (exit 141), which pipefail reports as the
  # pipeline status — turning a real "\"root\" execution" match into a false PASS.
  local logs
  logs="$(docker logs "$1" 2>&1)"
  if grep -q '"root" execution' <<<"$logs"; then
    fail "$2: container logged a \"root\" execution error (privilege drop failed)"
  fi
  echo "PASS: $2: no \"root\" execution error in logs"
}

wait_ready() { # <container>
  # Gate on TCP, not the Unix socket. The base entrypoint runs a transient
  # socket-only server (listen_addresses='') during initdb / init scripts;
  # `pg_isready` over the socket would race against that init-phase server and
  # then see the socket vanish when it stops for the real startup. Only the
  # real server binds TCP (listen_addresses='*'), so `-h 127.0.0.1` waits for it.
  for _ in $(seq 1 90); do
    if docker exec "$1" pg_isready -h 127.0.0.1 -U postgres >/dev/null 2>&1; then return 0; fi
    sleep 1
  done
  fail "$1: postgres did not become ready within 90s"
}

psql1() { # <container> <sql> — first scalar of a single-row result
  # Over TCP (127.0.0.1) so we never talk to the init-phase socket server; the
  # base image uses scram-sha-256 for host connections when POSTGRES_PASSWORD
  # is set, hence PGPASSWORD.
  docker exec -e PGPASSWORD=smoke "$1" psql -h 127.0.0.1 -U postgres -t -A -c "$2"
}

postmaster_user() { # <container> — OS user the postmaster process runs as
  # The base wrapper.sh is PID 1 (a root supervisor); the postmaster is a
  # managed child. Read its PID from postmaster.pid and stat its /proc entry.
  docker exec "$1" sh -c 'stat -c "%U" /proc/"$(head -1 /var/lib/postgresql/data/postmaster.pid)"'
}

cleanup() {
  local rc=$?
  if [[ $rc -ne 0 ]]; then
    local c
    for c in "$C_BASE" "$C_ENV" "$C_SIG"; do
      if docker container inspect "$c" >/dev/null 2>&1; then
        echo "----- docker logs: $c (tail) -----" >&2
        docker logs "$c" 2>&1 | tail -60 >&2 || true
      fi
    done
  fi
  docker rm -f "$C_BASE" "$C_ENV" "$C_SIG" >/dev/null 2>&1 || true
  docker volume rm "$VOL_SIG" >/dev/null 2>&1 || true
}
trap cleanup EXIT

# Clear any leftovers from a prior aborted run before starting.
docker rm -f "$C_BASE" "$C_ENV" "$C_SIG" >/dev/null 2>&1 || true
docker volume rm "$VOL_SIG" >/dev/null 2>&1 || true

echo "=== Image: $IMG  (PG $PG, SHAREDIR $SHAREDIR) ==="

# ===========================================================================
# 1 + 2. Baseline container — WXYC_PG_EXTRA_ARGS unset (runtime-identical path)
# ===========================================================================
echo "--- [1/4] baked dictionary integrity + [2/4] unset baseline ---"
docker run -d --name "$C_BASE" -e POSTGRES_PASSWORD=smoke "$IMG" >/dev/null
wait_ready "$C_BASE"

# 1. Dictionary integrity (rules + version file baked in, on disk, and usable).
EXPECTED_RULES_SHA="$(sha256_of "${DATA_DIR}/wxyc_unaccent.rules")"
EXPECTED_VERSION_SHA="$(sha256_of "${DATA_DIR}/wxyc_unaccent.version")"
ACTUAL_RULES_SHA="$(docker exec "$C_BASE" sha256sum "${SHAREDIR}/tsearch_data/wxyc_unaccent.rules" | awk '{print $1}')"
ACTUAL_VERSION_SHA="$(docker exec "$C_BASE" sha256sum "${SHAREDIR}/tsearch_data/wxyc_unaccent.version" | awk '{print $1}')"
assert_eq "wxyc_unaccent.rules SHA-256"   "$EXPECTED_RULES_SHA"   "$ACTUAL_RULES_SHA"
assert_eq "wxyc_unaccent.version SHA-256" "$EXPECTED_VERSION_SHA" "$ACTUAL_VERSION_SHA"
assert_eq "rules file non-empty (pg_stat_file)" "t" \
  "$(psql1 "$C_BASE" "SELECT (pg_stat_file('${SHAREDIR}/tsearch_data/wxyc_unaccent.rules')).size > 0;")"
assert_eq "version file non-empty (pg_stat_file)" "t" \
  "$(psql1 "$C_BASE" "SELECT (pg_stat_file('${SHAREDIR}/tsearch_data/wxyc_unaccent.version')).size > 0;")"
# Set up the dictionary in its own call (DROP IF EXISTS keeps it idempotent),
# discarding the command-tag chatter, then assert a clean scalar from SELECT.
docker exec -e PGPASSWORD=smoke "$C_BASE" psql -h 127.0.0.1 -U postgres -q -c \
  "CREATE EXTENSION IF NOT EXISTS unaccent; DROP TEXT SEARCH DICTIONARY IF EXISTS wxyc_unaccent; CREATE TEXT SEARCH DICTIONARY wxyc_unaccent (TEMPLATE = unaccent, RULES = 'wxyc_unaccent');" >/dev/null
assert_eq "ts_lexize('wxyc_unaccent','café')" "{cafe}" \
  "$(psql1 "$C_BASE" "SELECT ts_lexize('wxyc_unaccent', 'café');")"

# 2. Runtime-identical baseline: privilege drop, SSL, TCP bind all intact with no env.
assert_eq "unset: postmaster runs as postgres" "postgres" "$(postmaster_user "$C_BASE")"
assert_eq "unset: SSL on"                       "on"       "$(psql1 "$C_BASE" 'SHOW ssl;')"
assert_eq "unset: listen_addresses"             "*"        "$(psql1 "$C_BASE" 'SHOW listen_addresses;')"
assert_eq "unset: shared_buffers is stock default" "128MB"  "$(psql1 "$C_BASE" 'SHOW shared_buffers;')"
assert_no_root_error "$C_BASE" "unset"

# ===========================================================================
# 3. WXYC_PG_EXTRA_ARGS feature — full 6-flag string
# ===========================================================================
echo "--- [3/4] WXYC_PG_EXTRA_ARGS applies (multi-flag, last-wins, command line) ---"
docker run -d --name "$C_ENV" -e POSTGRES_PASSWORD=smoke -e WXYC_PG_EXTRA_ARGS="$EXTRA_ARGS_FULL" "$IMG" >/dev/null
wait_ready "$C_ENV"

# Safety still holds with the env set.
assert_eq "env: postmaster runs as postgres" "postgres" "$(postmaster_user "$C_ENV")"
assert_eq "env: SSL on"                       "on"       "$(psql1 "$C_ENV" 'SHOW ssl;')"
assert_no_root_error "$C_ENV" "env"

# All six flags word-split into distinct, applied settings.
assert_eq "env: shared_buffers (last-wins over 128MB)" "2GB"  "$(psql1 "$C_ENV" 'SHOW shared_buffers;')"
assert_eq "env: effective_cache_size"                  "6GB"  "$(psql1 "$C_ENV" 'SHOW effective_cache_size;')"
assert_eq "env: work_mem"                              "16MB" "$(psql1 "$C_ENV" 'SHOW work_mem;')"
assert_eq "env: maintenance_work_mem"                  "512MB" "$(psql1 "$C_ENV" 'SHOW maintenance_work_mem;')"
assert_eq "env: random_page_cost"                      "1.1"  "$(psql1 "$C_ENV" 'SHOW random_page_cost;')"
assert_eq "env: effective_io_concurrency"              "200"  "$(psql1 "$C_ENV" 'SHOW effective_io_concurrency;')"

# Precedence claim locked in: the value is sourced from the command line, not a
# default / config file. Proves the appended `-c` really overrode the default.
assert_eq "env: shared_buffers source" "command line" \
  "$(psql1 "$C_ENV" "SELECT source FROM pg_settings WHERE name = 'shared_buffers';")"

# ===========================================================================
# 4. Process-model + restart integrity
#
# The issue's signal/WAL criterion assumes the base wrapper.sh `exec`s the
# postgres entrypoint (postgres becomes PID 1 and receives signals directly).
# That is FALSE for the pinned base: wrapper.sh's last line runs
# `docker-entrypoint.sh "$@"` WITHOUT `exec` and it installs no signal traps, so
# PID 1 (wrapper.sh, bash) never forwards SIGINT/SIGTERM to the postmaster
# descendant. `docker stop` therefore falls back to Docker's SIGKILL and the next
# boot does a trivial ("redo is not required") crash recovery. That is a property
# of the BASE image — verified identical on the unmodified base — not of this
# overlay, and making it clean would require diverging from the base (out of
# scope; the image is a pure overlay, and it must stay runtime-identical to the
# base when WXYC_PG_EXTRA_ARGS is unset).
#
# What this overlay MUST preserve, and what we assert here, is that it inserts NO
# process layer of its own: wxyc-entrypoint.sh `exec`s wrapper.sh, so PID 1 stays
# wrapper.sh and the postmaster is not stranded behind an EXTRA non-forwarding
# parent (the real intent behind the issue's signal criterion). Plus: the
# postmaster receives exactly the composed argv, and the tuning survives a full
# container replacement (Railway redeploy).
# ===========================================================================
echo "--- [4/4] process-model + redeploy integrity ---"
docker volume create "$VOL_SIG" >/dev/null
docker run -d --name "$C_SIG" -v "${VOL_SIG}:/var/lib/postgresql/data" \
  -e POSTGRES_PASSWORD=smoke -e WXYC_PG_EXTRA_ARGS="-c shared_buffers=2GB" "$IMG" >/dev/null
wait_ready "$C_SIG"

# exec passthrough: PID 1 is the base wrapper.sh, not our wxyc-entrypoint.sh.
# This is the guard behind the issue's signal criterion — our layer adds no
# parent that could strand the postmaster.
assert_eq "PID 1 is the base wrapper.sh (exec passthrough)" "wrapper.sh" \
  "$(docker exec "$C_SIG" cat /proc/1/comm)"

# The postmaster receives exactly its OWN base's re-declared CMD, then the
# appended extra flag (last-wins ordering is visible in the argv itself — extra
# after the CMD). The two pinned bases differ (verified via
# `docker buildx imagetools inspect`): pg17 sets listen_addresses on the command
# line, pg16 provides it via postgresql.conf. Encode each base's CMD here so this
# assertion catches drift if a future base-digest refresh changes it without the
# Dockerfile CMD (and this case) being updated in lockstep.
case "$PG" in
  16) EXPECTED_BASE_CMD="postgres --port=5432" ;;
  17) EXPECTED_BASE_CMD="postgres -p 5432 -c listen_addresses=*" ;;
  *)  fail "unknown PG major $PG — add its base CMD (see Dockerfile.pg$PG) to the case above" ;;
esac
assert_eq "postmaster argv = base CMD + appended extra" \
  "$EXPECTED_BASE_CMD -c shared_buffers=2GB" \
  "$(docker exec "$C_SIG" sh -c 'tr "\0" " " < /proc/"$(head -1 /var/lib/postgresql/data/postmaster.pid)"/cmdline | sed "s/ *\$//"')"

# Survives a full container replacement — the faithful model of a Railway redeploy
# (which recreates the container against the persisted volume, not a stop/start).
# `docker rm -f` is SIGKILL with no stop-grace wait; the base ignores stop signals
# anyway (see the section header), so this loses no fidelity and ~10s of wall time.
docker rm -f "$C_SIG" >/dev/null
docker run -d --name "$C_SIG" -v "${VOL_SIG}:/var/lib/postgresql/data" \
  -e POSTGRES_PASSWORD=smoke -e WXYC_PG_EXTRA_ARGS="-c shared_buffers=2GB" "$IMG" >/dev/null
wait_ready "$C_SIG"
assert_eq "redeploy: shared_buffers still applied" "2GB" "$(psql1 "$C_SIG" 'SHOW shared_buffers;')"

echo "=== ALL SMOKE ASSERTIONS PASSED (PG $PG) ==="
