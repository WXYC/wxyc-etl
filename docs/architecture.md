# Architecture

## Workspace Layout

Cargo workspace with two members:

| Crate | Path | Purpose |
|---|---|---|
| `wxyc-etl` | `wxyc-etl/` | Pure Rust library. Used by other Rust ETL repos (discogs-xml-converter, musicbrainz-cache, wikidata-cache) via `[dependencies] wxyc-etl = { path = "../wxyc-etl" }`. |
| `wxyc-etl-python` | `wxyc-etl-python/` | PyO3 extension that re-exports a curated subset to Python as `wxyc_etl.text`, `wxyc_etl.fuzzy`, `wxyc_etl.parser`, `wxyc_etl.state`, `wxyc_etl.import_utils`, `wxyc_etl.schema`. Built with maturin. |

The Python crate lives in the same workspace so it shares the lockfile with the Rust crate it wraps; this guarantees the bindings always match the underlying library at the byte level.

## Modules (`wxyc-etl/src/`)

| Module | Purpose |
|---|---|
| `text` | WX-2 charter forms (`to_storage_form`, `to_match_form`, `to_ascii_form`) and the cross-cache-identity layer (`to_identity_match_form` + variants in `text::identity`); compilation detection (`is_compilation_artist`); multi-artist split with optional contextual hints (`split_artist_name`, `split_artist_name_contextual`); batch variants in `text::batch`; file-backed `ArtistFilter` / `TitleFilter` in `text::filter`. |
| `pg` | `BatchCopier` and friends for `COPY TEXT` bulk loading, FK-ordered flush, dedup tracking, admin helpers (`SET UNLOGGED` / `SET LOGGED`). Backed by sync `postgres` crate. |
| `pipeline` | Generic scanner → rayon → writer parallel framework (`PipelineRunner`, `PipelineOutput` trait). Used by the streaming Rust filters to pipe huge dumps through worker pools without building intermediate vectors. |
| `csv_writer` | `MultiCsvWriter` — write to many CSVs in parallel from a single producer. |
| `sqlite` | SQLite helpers: FTS5 setup, performance pragmas, batch insert wrappers. |
| `state` | `PipelineState` — JSON state file at the workspace root, tracks completed pipeline steps for `--resume`. `state::introspect` derives a state file from DB introspection (schema present, row counts, index presence) when no file exists. |
| `import` | Artist/track dedup helpers and column mapping for the discogs-etl import path. |
| `schema` | Table-name constants for the consumer databases (`schema::library`, `schema::discogs`, `schema::musicbrainz`, `schema::wikidata`, `schema::entity`). Single source of truth so consumer Rust code never hard-codes table names. |
| `fuzzy` | `LibraryIndex` (token-set + Jaro-Winkler scoring), batch filter via normalize + set lookup, classification metrics. |
| `parser` | Streaming MySQL `INSERT INTO ... VALUES (...)` tuple parser used to read tubafrenzy SQL dumps without loading them into memory. |
| `logger` | Sentry + structured JSON logging (`tracing` + `tracing-subscriber` JSON). See [`docs/observability.md`](observability.md). |
| `cli` | Shared `clap` argument groups (`DatabaseArgs`, `ResumableBuildArgs`, `ImportArgs`) and `resolve_database_url(args, env_name)` for the cache-builder CLI convention. See [`docs/cli-convention.md`](cli-convention.md). |

`lib.rs` re-exports each module unchanged: consumers do `use wxyc_etl::text::to_match_form`.
