# Observability

Every ETL binary should call `wxyc_etl::logger::init` (Rust) or `wxyc_etl.logger.init_logger` (Python) once at startup. Both emit one JSON object per line on stderr with four required tags so logs are uniformly aggregatable across pipelines:

| Tag | Source | Example |
|---|---|---|
| `repo` | passed to `init` | `"musicbrainz-cache"` |
| `tool` | passed to `init` | `"musicbrainz-cache build"` |
| `step` | per-event field | `"import"`, `"resolve"`, `"copy"` |
| `run_id` | UUIDv4 (or override) | `"4eb6f1b7-..."` |

When the `SENTRY_DSN` env var is set (or `sentry_dsn` is passed in), panics and `tracing::error!` events (Rust) / `logger.error` events (Python) are forwarded to Sentry tagged with the same fields. The Python logger no longer talks to Sentry: `sentry_dsn` is accepted and ignored there, and errors go to PostHog (below).

## Rust

```rust
use wxyc_etl::logger::{self, LoggerConfig};

fn main() {
    let _guard = logger::init(LoggerConfig {
        repo: "musicbrainz-cache",
        tool: "musicbrainz-cache build",
        sentry_dsn: None,         // falls back to SENTRY_DSN env
        run_id: None,             // generates UUIDv4
    });

    tracing::info_span!("import", step = "import").in_scope(|| {
        tracing::info!(rows = 42, "loaded recordings");
    });
}
```

The guard must be held for the lifetime of `main` so Sentry flushes on drop.

## Python

```python
from wxyc_etl.logger import init_logger
import logging

guard = init_logger(repo="discogs-etl", tool="discogs-etl daily-sync")

log = logging.getLogger(__name__)
log.info("loaded recordings", extra={"step": "import", "rows": 42})
```

### PostHog error tracking

When `POSTHOG_ERRORS_API_KEY` is set and the optional `posthog` extra is installed (`pip install wxyc-etl[posthog]`), `init_logger` also attaches a handler that sends ERROR records to PostHog error tracking Records with `exc_info` go through `capture_exception`; others become a synthesized `$exception` event whose `$exception_fingerprint` is the single string `"<logger name>:<unformatted message>"` (PostHog types the field as a string; an event that sends a list is still ingested, with `$cymbal_errors`, and probably counts against the cap, but never becomes an issue). The event follows the error-tracking contract ([`patterns/error-tracking.md`](https://github.com/WXYC/wiki/blob/main/patterns/error-tracking.md) in WXYC/wiki): the message-only entry has `type: "Message"` and mechanism `{type: "generic", handled: true, synthetic: true}`; every event carries `service`, `environment`, `logger` (the record's logger name), `repo`, `tool`, `step`, `run_id`, a level (`$exception_level: "error"`; on the `capture_exception` path only when the installed posthog is older than 7.65.0, which sets it itself) and `$process_person_profile: false`; and `distinct_id` is the `service` value. `service` defaults to `tool` in kebab-case (`"discogs-etl daily-sync"` becomes `discogs-etl-daily-sync`); `environment` defaults to the `WXYC_ENVIRONMENT` env var, else `production`, and is normalised to `production`, `staging` or `development` (`prod`, `stage`, `dev` and `local` are aliases; an unknown value passes through with a one-time warning). The handler is built on the first `init_logger` call, so budget arguments on later calls are ignored (a different pair logs a warning). Both are `init_logger` keyword arguments and are also added to the JSON log lines. `POSTHOG_HOST` overrides the default `https://us.i.posthog.com`.

A same-fingerprint dedupe window (`POSTHOG_ERRORS_DEDUPE_WINDOW_SECONDS`, 60 s, applied before the bucket) and a token bucket cap volume. The default is the scheduled-run shape: capacity `posthog_run_ceiling` (default 10; pass 2 for a job that runs more than 4 times a day) and no refill. A **long-running process must pass `posthog_daily_ceiling`** (for example 100), which selects capacity 20 refilling at ceiling / 86,400 per second; without it the bucket would go silent for good after 10 errors. The posthog extra requires `posthog>=6.9`. Without the key or the extra, nothing is installed; the SDK's own `posthog` and `backoff` loggers are never captured.

A process that also runs wxyc-fastapi's `init_error_tracking` (the semantic-index API) must pass `posthog_errors=False` to `init_logger`: both handlers read `POSTHOG_ERRORS_API_KEY`, so otherwise every ERROR reaches PostHog twice through two clients with separate token buckets and dedupe tables. With `False`, no handler is installed, no `Posthog` client is built, and the guard has nothing to flush. The default is `True`, so pipelines keep the handler.

When the handler is built, `init_logger` also chains `sys.excepthook` and `threading.excepthook` (once per process), because the handler alone sees only ERROR log records and an uncaught exception would otherwise reach PostHog nowhere once Sentry is dropped. The hook routes the exception through the same dedupe window and token bucket as handled records (logger name `uncaught`, same contract properties), flushes the client, then always calls the previous hook so the traceback and exit code are unchanged. Crashes carry `mechanism.handled=false`. An exception already captured from a log record is not sent again. `KeyboardInterrupt` and `SystemExit` are skipped (in threads, only `SystemExit`). With `posthog_errors=False`, no key, or no extra, the hook is not installed.

`init_logger` is idempotent — calling it again replaces the tag values without re-installing handlers. Drop the guard or call `guard.flush()` to drain the PostHog errors client before exit; a guard from a repeat call drains the same PostHog client (also registered via `atexit`).
