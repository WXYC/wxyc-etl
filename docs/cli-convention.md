# Cache-builder CLI convention

Every cache builder (`musicbrainz-cache`, `wikidata-cache`, `discogs-xml-converter`, `discogs-etl`) is shaped the same way:

| Subcommand | Flags | Purpose |
|---|---|---|
| `<tool> build` | `--database-url`, `--resume`, `--state-file`, `--data-dir` | populate / refresh the cache (resumable) |
| `<tool> import` | `--database-url`, `--fresh`, `--data-dir` | load a fresh dump |

Two rules per builder:

1. **`--database-url` is required, with an env fallback.** When the flag is absent, fall back to `DATABASE_URL_<UPPERCASE_NAME>` (e.g. `DATABASE_URL_MUSICBRAINZ`, `DATABASE_URL_WIKIDATA`). Use [`wxyc_etl::cli::resolve_database_url`] to enforce this:

   ```rust
   use clap::Parser;
   use wxyc_etl::cli::{resolve_database_url, DatabaseArgs, ResumableBuildArgs};

   #[derive(Parser)]
   struct BuildCli {
       #[command(flatten)]
       db: DatabaseArgs,
       #[command(flatten)]
       build: ResumableBuildArgs,
   }

   let cli = BuildCli::parse();
   let url = resolve_database_url(&cli.db, "DATABASE_URL_MUSICBRAINZ")?;
   ```

2. **Compose, don't redeclare.** Use `#[command(flatten)]` rather than copy-pasting flag definitions; the shared module is the source of truth.
