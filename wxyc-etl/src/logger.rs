//! Sentry + structured JSON logging for WXYC ETL pipelines.
//!
//! Every ETL binary should call [`init`] once at startup with a [`LoggerConfig`]
//! identifying the repo and tool. Logs emit as one JSON object per line with the
//! tags `repo`, `tool`, `step`, and `run_id` (the last set per-span via
//! `tracing` instrumentation). When `SENTRY_DSN` is set in the environment (or
//! provided in the config) panics and `tracing::error!` events are forwarded to
//! Sentry with the same tags.
//!
//! ## Usage
//!
//! ```no_run
//! use wxyc_etl::logger::{self, LoggerConfig};
//!
//! fn main() {
//!     let _guard = logger::init(LoggerConfig {
//!         repo: "musicbrainz-cache",
//!         tool: "musicbrainz-cache build",
//!         sentry_dsn: None,
//!         run_id: None,
//!     });
//!
//!     tracing::info_span!("import", step = "import").in_scope(|| {
//!         tracing::info!(rows = 42, "loaded recordings");
//!     });
//! }
//! ```

use std::sync::OnceLock;

use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

/// Configuration for [`init`]. Pass `None` for `sentry_dsn` to fall back to the
/// `SENTRY_DSN` environment variable; pass `None` for `run_id` to generate a
/// fresh UUIDv4 for the run.
pub struct LoggerConfig {
    /// Repository name. Set as the Sentry tag `repo`.
    pub repo: &'static str,
    /// Tool name (typically `<repo> <subcommand>`). Set as the Sentry tag `tool`.
    pub tool: &'static str,
    /// Sentry DSN. `None` falls back to `SENTRY_DSN` env var; if neither is set
    /// Sentry is disabled but JSON logging still initializes.
    pub sentry_dsn: Option<String>,
    /// Run identifier. `None` generates a UUIDv4. Set as the Sentry tag `run_id`.
    pub run_id: Option<String>,
}

/// RAII guard returned by [`init`]. Drop it at the end of `main` to flush
/// pending Sentry events before exit.
pub struct LoggerGuard {
    _sentry: Option<sentry::ClientInitGuard>,
}

static INIT: OnceLock<()> = OnceLock::new();

/// Initialize Sentry and JSON logging. Safe to call once per process; later
/// calls are a no-op (and return a `LoggerGuard` whose Sentry slot is empty).
pub fn init(config: LoggerConfig) -> LoggerGuard {
    let run_id = config
        .run_id
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    let dsn = config
        .sentry_dsn
        .or_else(|| std::env::var("SENTRY_DSN").ok());

    let mut sentry_guard = None;
    if let Some(dsn) = dsn.as_deref().filter(|s| !s.is_empty()) {
        let guard = sentry::init((
            dsn.to_string(),
            sentry::ClientOptions {
                release: sentry::release_name!(),
                attach_stacktrace: true,
                ..Default::default()
            },
        ));
        sentry::configure_scope(|scope| {
            scope.set_tag("repo", config.repo);
            scope.set_tag("tool", config.tool);
            scope.set_tag("run_id", &run_id);
        });
        sentry_guard = Some(guard);
    }

    if INIT.set(()).is_ok() {
        let _ = tracing_log::LogTracer::init();

        let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

        // Write to stderr so the contract matches `env_logger` and POSIX
        // convention: stdout is for the program's data output, stderr is for
        // diagnostics. Existing tests across consumer repos read stderr for
        // log lines and break if logs land on stdout instead.
        let json_layer = tracing_subscriber::fmt::layer()
            .json()
            .with_current_span(true)
            .with_span_list(false)
            .with_target(true)
            .with_writer(std::io::stderr);

        let registry = tracing_subscriber::registry()
            .with(env_filter)
            .with(json_layer);

        if sentry_guard.is_some() {
            let _ = registry.with(sentry_tracing::layer()).try_init();
        } else {
            let _ = registry.try_init();
        }

        // Re-set repo/tool/run_id as tracing fields on the root span so every
        // event carries them even outside a user-defined span.
        let span = tracing::info_span!(
            "wxyc_etl",
            repo = %config.repo,
            tool = %config.tool,
            run_id = %run_id,
        );
        let _ = span.entered();
    }

    LoggerGuard {
        _sentry: sentry_guard,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_without_dsn_does_not_panic() {
        // Safety net: explicitly clear SENTRY_DSN so the test is deterministic
        // regardless of the surrounding env.
        std::env::remove_var("SENTRY_DSN");
        let _guard = init(LoggerConfig {
            repo: "test-repo",
            tool: "test-tool",
            sentry_dsn: None,
            run_id: Some("test-run".into()),
        });
    }

    #[test]
    fn init_generates_run_id_when_none() {
        std::env::remove_var("SENTRY_DSN");
        // Calling without a run_id should not panic; UUID generation is the
        // only side effect we can observe externally without a custom writer.
        let _guard = init(LoggerConfig {
            repo: "test-repo",
            tool: "test-tool",
            sentry_dsn: None,
            run_id: None,
        });
    }

    #[test]
    fn init_idempotent() {
        std::env::remove_var("SENTRY_DSN");
        let _g1 = init(LoggerConfig {
            repo: "r1",
            tool: "t1",
            sentry_dsn: None,
            run_id: None,
        });
        let _g2 = init(LoggerConfig {
            repo: "r2",
            tool: "t2",
            sentry_dsn: None,
            run_id: None,
        });
    }
}
