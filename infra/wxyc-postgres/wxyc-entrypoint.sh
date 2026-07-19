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
#   WXYC_PG_EXTRA_ARGS unset/empty -> runtime-identical to the base image
#     (same postgres argv, process model, privilege drop, and SSL).
#   WXYC_PG_EXTRA_ARGS set         -> its words are appended as extra `postgres`
#     argv after the inherited CMD. PostgreSQL command-line parsing is last-wins,
#     so an appended `-c foo=bar` overrides an inherited default of the same key.
#
# Safety
# ------
#   - `exec` (not a child call) keeps the base wrapper.sh as PID 1 and inserts
#     no extra parent of our own — the postmaster is not stranded behind an
#     additional non-forwarding process, so signal semantics are exactly the
#     base's. (Note: this pinned base's wrapper.sh runs docker-entrypoint.sh
#     WITHOUT exec and installs no signal traps, so it does not itself forward
#     SIGINT/SIGTERM to the postmaster; that is a base-image property this
#     overlay preserves unchanged, not something it introduces.)
#   - Absolute path to wrapper.sh so a base-image PATH change can't silently
#     break the chain.
#   - The value is spliced in via `exec … ${VAR}` and word-split into additional
#     `postgres` argv entries — never through `sh -c "… $VAR"`. So the worst a
#     malicious/typo'd value can do is inject bounded postgres flags, NOT shell
#     commands. Do not "harden" this into a shell-form; that reintroduces
#     command injection. (Setting the var already requires Railway service
#     access, so this adds no new attack surface either way.)
#   - Word-splitting does NOT honor quotes inside the value, so each flag and
#     its value must be space-free (e.g. `-c shared_buffers=2GB`). A value with
#     an embedded space — e.g. `-c log_line_prefix=%m [%p]` — would be split
#     mid-value and hand postgres a stray positional arg, crash-looping startup.
#     Keep to simple `-c key=value` flags with space-free values (documented in
#     docs/wxyc-postgres-image.md).
set -e

# Disable pathname expansion (globbing) before the unquoted expansion below, so a
# value containing a glob metacharacter (`*`, `?`, `[...]`) is passed through
# literally rather than expanded against the entrypoint's working directory.
# `set -f` does not affect word-splitting, which is the behavior we DO want here.
set -f

# Unquoted ${WXYC_PG_EXTRA_ARGS} on purpose: it is a flag list and must
# word-split into separate argv entries. Unset/empty expands to nothing, so
# there is no stray empty argument when the knob is not in use. (See the "Safety"
# note above on why values must be space-free.)
# shellcheck disable=SC2086  # intentional word-splitting of the flag list
exec /usr/local/bin/wrapper.sh "$@" ${WXYC_PG_EXTRA_ARGS}
