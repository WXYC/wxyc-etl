#!/usr/bin/env bash
#
# wxyc-entrypoint.sh — thin wrapper over the base postgres-ssl entrypoint that
# appends optional per-service Postgres tuning flags from $WXYC_PG_EXTRA_ARGS.
#
# Why this exists
# ---------------
# Railway's per-service "Custom Start Command" is the obvious per-service tuning
# lever, but it *replaces* the container entrypoint — bypassing the base image's
# wrapper.sh -> docker-entrypoint.sh chain that (a) drops root -> the `postgres`
# user via gosu (keyed on argv[0] == "postgres") and (b) provisions SSL into
# postgresql.conf. Bypassing that chain crash-loops with
#   "root" execution of the PostgreSQL server is not permitted
# (verified live on 2026-07-18; see WXYC/discogs-etl#314). This wrapper runs
# *before* the base chain and only appends argv, so the privilege drop, SSL,
# and `-c listen_addresses=*` (the only flag that binds TCP) all still fire.
#
# Contract
# --------
#   WXYC_PG_EXTRA_ARGS unset/empty -> byte-identical to the base image.
#   WXYC_PG_EXTRA_ARGS set         -> its words are appended as extra `postgres`
#     argv after the inherited CMD. PostgreSQL command-line parsing is last-wins,
#     so an appended `-c foo=bar` overrides an inherited default of the same key.
#
# Safety
# ------
#   - `exec` (not a child call) keeps postgres as PID 1, so SIGINT/SIGTERM reach
#     it directly -> clean checkpoint + shutdown, no WAL replay on next start.
#   - Absolute path to wrapper.sh so a base-image PATH change can't silently
#     break the chain.
#   - The value is spliced in via `exec … ${VAR}` and word-split into additional
#     `postgres` argv entries — never through `sh -c "… $VAR"`. So the worst a
#     malicious/typo'd value can do is inject bounded postgres flags, NOT shell
#     commands. Do not "harden" this into a shell-form; that reintroduces
#     command injection. (Setting the var already requires Railway service
#     access, so this adds no new attack surface either way.)
set -e

# Unquoted ${WXYC_PG_EXTRA_ARGS} on purpose: it is a flag list and must
# word-split into separate argv entries. Unset/empty expands to nothing, so
# there is no stray empty argument when the knob is not in use.
# shellcheck disable=SC2086  # intentional word-splitting of the flag list
exec /usr/local/bin/wrapper.sh "$@" ${WXYC_PG_EXTRA_ARGS}
