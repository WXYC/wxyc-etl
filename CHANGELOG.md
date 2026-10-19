# Changelog

## 0.10.3

- Python logger: when `init_logger` builds the PostHog errors handler it also chains `sys.excepthook` and `threading.excepthook`, so an uncaught exception (including a crashed thread) is reported through the handler's dedupe window and token bucket (one gate with handled records), the client is flushed, and the previous hook always runs, even if the flush is interrupted. Crashes are sent with `mechanism.handled=false` (type `excepthook` or `threading.excepthook`; the private `_capture_metadata` kwarg first appears in posthog-python 7.65.0; versions 6.9 through 7.64 ignore it and send the crash as handled). An exception already captured from a log record (`log.exception(...)` then `raise`) is not sent a second time. `KeyboardInterrupt` and `SystemExit` are not reported (in threads, only `SystemExit`). Nothing is installed without the key or the `posthog` extra, or with `posthog_errors=False` (#161).

## 0.10.2

- Python logger: the PostHog error-tracking handler now follows the shared error-tracking contract (`patterns/error-tracking.md` in WXYC/wiki). Message-only events use `type: "Message"` with a generic synthetic mechanism and `$exception_level: "error"`; every event carries `service`, `environment` and `logger`; `distinct_id` is the `service` value (#159).
- `init_logger` and `LoggerConfig` gain `service`, `environment`, `posthog_run_ceiling` and `posthog_daily_ceiling`. The default PostHog budget drops from capacity 100 with 30/min refill to capacity 10 with no refill (a scheduled run). Long-running processes must pass `posthog_daily_ceiling` to get a capacity-20 bucket that refills at ceiling / 86,400 per second.
- The `posthog` extra now requires `posthog>=6.9`.
- Python logger: `environment` is normalised to the contract's vocabulary (`prod` becomes `production`, `stage` becomes `staging`, `dev` and `local` become `development`). Unknown values pass through unchanged with a single warning (#159).
- Python logger: the PostHog handler is built once, on the first `init_logger` call that builds one. A later call that passes different `posthog_run_ceiling` or `posthog_daily_ceiling` values logs a warning that the ceilings are ignored (#159).
