//! Shared `clap` argument groups for WXYC cache-builder CLIs.
//!
//! Every cache builder (musicbrainz-cache, wikidata-cache, discogs-xml-converter,
//! discogs-etl) accepts the same shape of flags. The structs in this module
//! capture that contract so each tool composes them via `#[clap(flatten)]`
//! rather than redeclaring `--database-url`, `--resume`, `--data-dir`, etc.
//!
//! ## Database URL convention
//!
//! Every builder MUST:
//!   1. Accept `--database-url <URL>`.
//!   2. Fall back to the env var `DATABASE_URL_<UPPERCASE_NAME>` (e.g.
//!      `DATABASE_URL_MUSICBRAINZ`) when the flag is absent.
//!   3. Error clearly when neither is set.
//!
//! Use [`resolve_database_url`] to enforce this:
//!
//! ```no_run
//! use clap::Parser;
//! use wxyc_etl::cli::{resolve_database_url, DatabaseArgs};
//!
//! #[derive(Parser)]
//! struct Cli {
//!     #[command(flatten)]
//!     db: DatabaseArgs,
//! }
//!
//! let cli = Cli::parse();
//! let url = resolve_database_url(&cli.db, "DATABASE_URL_MUSICBRAINZ").unwrap();
//! ```
//!
//! ## Subcommand convention
//!
//! Every cache builder exposes two subcommands with consistent shapes:
//!
//!   * `<tool> build [--resume]` — populate the cache (resumable). Composes
//!     [`ResumableBuildArgs`] with [`DatabaseArgs`].
//!   * `<tool> import [--fresh]` — load a fresh dump into the database.
//!     Composes [`ImportArgs`] with [`DatabaseArgs`].

use std::path::PathBuf;

use clap::Args;
use thiserror::Error;

/// `--database-url` flag with no compile-time env binding (each tool's env
/// name is passed at resolve time).
#[derive(Args, Debug, Clone)]
pub struct DatabaseArgs {
    /// PostgreSQL connection URL. Falls back to the tool's
    /// `DATABASE_URL_<NAME>` env var when omitted (see [`resolve_database_url`]).
    #[arg(long)]
    pub database_url: Option<String>,
}

/// Flags shared by `<tool> build` subcommands.
#[derive(Args, Debug, Clone)]
pub struct ResumableBuildArgs {
    /// Resume from the existing state file instead of starting fresh.
    #[arg(long)]
    pub resume: bool,

    /// Path to the JSON state file used for `--resume` checkpoints.
    #[arg(long, default_value = "./state.json")]
    pub state_file: PathBuf,

    /// Working data directory (intermediate CSVs, downloads, etc.).
    #[arg(long, default_value = "./data")]
    pub data_dir: PathBuf,
}

/// Flags shared by `<tool> import` subcommands.
#[derive(Args, Debug, Clone)]
pub struct ImportArgs {
    /// Drop and recreate the schema before importing.
    #[arg(long)]
    pub fresh: bool,

    /// Working data directory (the dump to read from).
    #[arg(long, default_value = "./data")]
    pub data_dir: PathBuf,
}

/// Errors returned by [`resolve_database_url`].
#[derive(Debug, Error, PartialEq, Eq)]
pub enum CliError {
    /// Neither the `--database-url` flag nor the fallback env var was set.
    #[error("no database URL: set --database-url or the {env_name} environment variable")]
    MissingDatabaseUrl { env_name: String },
}

/// Resolve the database URL from CLI flag → env var, in that order.
///
/// `env_name` is the tool's chosen fallback (e.g. `"DATABASE_URL_MUSICBRAINZ"`).
/// Returns [`CliError::MissingDatabaseUrl`] when neither is set.
pub fn resolve_database_url(args: &DatabaseArgs, env_name: &str) -> Result<String, CliError> {
    if let Some(url) = args.database_url.as_deref().filter(|s| !s.is_empty()) {
        return Ok(url.to_string());
    }
    if let Ok(url) = std::env::var(env_name) {
        if !url.is_empty() {
            return Ok(url);
        }
    }
    Err(CliError::MissingDatabaseUrl {
        env_name: env_name.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser)]
    struct TestCli {
        #[command(flatten)]
        db: DatabaseArgs,
        #[command(flatten)]
        build: ResumableBuildArgs,
    }

    #[derive(Parser)]
    struct ImportCli {
        #[command(flatten)]
        db: DatabaseArgs,
        #[command(flatten)]
        import: ImportArgs,
    }

    #[test]
    fn parses_database_url_flag() {
        let cli =
            TestCli::try_parse_from(["tool", "--database-url", "postgres://localhost:5432/db"])
                .unwrap();
        assert_eq!(
            cli.db.database_url.as_deref(),
            Some("postgres://localhost:5432/db")
        );
    }

    #[test]
    fn build_args_default_values() {
        let cli = TestCli::try_parse_from(["tool"]).unwrap();
        assert!(!cli.build.resume);
        assert_eq!(cli.build.state_file, PathBuf::from("./state.json"));
        assert_eq!(cli.build.data_dir, PathBuf::from("./data"));
    }

    #[test]
    fn build_args_resume_flag() {
        let cli = TestCli::try_parse_from(["tool", "--resume"]).unwrap();
        assert!(cli.build.resume);
    }

    #[test]
    fn import_args_fresh_flag() {
        let cli = ImportCli::try_parse_from(["tool", "--fresh"]).unwrap();
        assert!(cli.import.fresh);
    }

    #[test]
    fn resolve_uses_flag_when_present() {
        let args = DatabaseArgs {
            database_url: Some("postgres://flag-host/db".into()),
        };
        // The env var should be ignored when the flag is set.
        std::env::set_var("DATABASE_URL_TEST_FLAG_WINS", "postgres://env-host/db");
        let url = resolve_database_url(&args, "DATABASE_URL_TEST_FLAG_WINS").unwrap();
        std::env::remove_var("DATABASE_URL_TEST_FLAG_WINS");
        assert_eq!(url, "postgres://flag-host/db");
    }

    #[test]
    fn resolve_falls_back_to_env() {
        let args = DatabaseArgs { database_url: None };
        std::env::set_var("DATABASE_URL_TEST_FALLBACK", "postgres://env-host/db");
        let url = resolve_database_url(&args, "DATABASE_URL_TEST_FALLBACK").unwrap();
        std::env::remove_var("DATABASE_URL_TEST_FALLBACK");
        assert_eq!(url, "postgres://env-host/db");
    }

    #[test]
    fn resolve_errors_when_both_missing() {
        let args = DatabaseArgs { database_url: None };
        std::env::remove_var("DATABASE_URL_TEST_MISSING");
        let err = resolve_database_url(&args, "DATABASE_URL_TEST_MISSING").unwrap_err();
        assert_eq!(
            err,
            CliError::MissingDatabaseUrl {
                env_name: "DATABASE_URL_TEST_MISSING".into()
            }
        );
    }

    #[test]
    fn resolve_treats_empty_flag_as_missing() {
        let args = DatabaseArgs {
            database_url: Some(String::new()),
        };
        std::env::set_var("DATABASE_URL_TEST_EMPTY_FLAG", "postgres://env-host/db");
        let url = resolve_database_url(&args, "DATABASE_URL_TEST_EMPTY_FLAG").unwrap();
        std::env::remove_var("DATABASE_URL_TEST_EMPTY_FLAG");
        assert_eq!(url, "postgres://env-host/db");
    }

    #[test]
    fn resolve_treats_empty_env_as_missing() {
        let args = DatabaseArgs { database_url: None };
        std::env::set_var("DATABASE_URL_TEST_EMPTY_ENV", "");
        let err = resolve_database_url(&args, "DATABASE_URL_TEST_EMPTY_ENV").unwrap_err();
        std::env::remove_var("DATABASE_URL_TEST_EMPTY_ENV");
        assert_eq!(
            err,
            CliError::MissingDatabaseUrl {
                env_name: "DATABASE_URL_TEST_EMPTY_ENV".into()
            }
        );
    }
}
