# Changelog

## 0.10.2

- Python logger: the PostHog error-tracking handler now follows the shared error-tracking contract (`patterns/error-tracking.md` in WXYC/wiki). Message-only events use `type: "Message"` with a generic synthetic mechanism and `$exception_level: "error"`; every event carries `service`, `environment` and `logger`; `distinct_id` is the `service` value (#159).
- `init_logger` and `LoggerConfig` gain `service`, `environment`, `posthog_run_ceiling` and `posthog_daily_ceiling`. The default PostHog budget drops from capacity 100 with 30/min refill to capacity 10 with no refill (a scheduled run). Long-running processes must pass `posthog_daily_ceiling` to get a capacity-20 bucket that refills at ceiling / 86,400 per second.
- The `posthog` extra now requires `posthog>=6.9`.
