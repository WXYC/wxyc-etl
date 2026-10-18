# Observability

Every ETL binary should call `wxyc_etl::logger::init` (Rust) or `wxyc_etl.logger.init_logger` (Python) once at startup. Both emit one JSON object per line on stderr with four required tags so logs are uniformly aggregatable across pipelines:

| Tag | Source | Example |
|---|---|---|
| `repo` | passed to `init` | `"musicbrainz-cache"` |
| `tool` | passed to `init` | `"musicbrainz-cache build"` |
| `step` | per-event field | `"import"`, `"resolve"`, `"copy"` |
| `run_id` | UUIDv4 (or override) | `"4eb6f1b7-..."` |

When the `SENTRY_DSN` env var is set (or `sentry_dsn` is passed in), panics and `tracing::error!` events (Rust) / `logger.error` events (Python) are forwarded to Sentry tagged with the same fields.

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

When `POSTHOG_ERRORS_API_KEY` is set and the optional `posthog` extra is installed (`pip install wxyc-etl[posthog]`), `init_logger` also attaches a handler that sends ERROR records to PostHog error tracking alongside Sentry. Records with `exc_info` go through `capture_exception`; others become a synthesized `$exception` event with fingerprint `[logger name, unformatted message]`. `repo`, `tool`, `step`, and `run_id` ride along as properties. `POSTHOG_HOST` overrides the default `https://us.i.posthog.com`. A token bucket (`POSTHOG_ERRORS_BUCKET_CAPACITY` / `POSTHOG_ERRORS_REFILL_PER_SECOND`) and a same-fingerprint dedupe window (`POSTHOG_ERRORS_DEDUPE_WINDOW_SECONDS`) cap volume. Without the key or the extra, nothing is installed; the SDK's own `posthog` and `backoff` loggers are never captured.

`init_logger` is idempotent — calling it again replaces the tag values without re-installing handlers. Drop the guard or call `guard.flush()` to drain Sentry before exit (also registered via `atexit`).
