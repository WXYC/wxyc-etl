//! Rust ↔ Postgres byte-equality parity for the four identity-match entry
//! points. Reads `tests/fixtures/identity_normalization_cases.csv`, runs each
//! row through the corresponding Rust function and the corresponding
//! plpgsql function, asserts byte-equal.
//!
//! Marked `#[ignore]` so plain `cargo test` skips it. CI runs the test in
//! the `test-postgres` job, which installs the `wxyc_unaccent` rules file
//! into the service container's tsearch_data directory before invoking
//! `cargo test -- --include-ignored`. Locally:
//!
//! ```sh
//! # one-time setup (writable Homebrew tsearch_data):
//! bash scripts/install_wxyc_unaccent.sh
//!
//! TEST_DATABASE_URL=postgresql://... \
//!   cargo test --test postgres_parity_test -- --include-ignored
//! ```
//!
//! Skip semantics: with `TEST_DATABASE_URL` unset, the test returns early
//! (matching the existing pg_error_tests pattern).

use std::fs;
use std::path::PathBuf;

use postgres::Client;
use wxyc_etl::text::{
    to_identity_match_form, to_identity_match_form_title,
    to_identity_match_form_with_disambiguator_strip, to_identity_match_form_with_punctuation,
};

#[derive(Debug)]
struct Row {
    line_no: usize,
    input: String,
    expected: String,
    variant: String,
    category: String,
}

fn test_db_url() -> Option<String> {
    std::env::var("TEST_DATABASE_URL").ok()
}

fn workspace_root() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop();
    p
}

fn fixture_path() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("tests/fixtures/identity_normalization_cases.csv");
    p
}

fn functions_sql_path() -> PathBuf {
    let mut p = workspace_root();
    p.push("data/wxyc_identity_match_functions.sql");
    p
}

fn parse_fixture() -> Vec<Row> {
    let text = fs::read_to_string(fixture_path()).expect("fixture missing");
    let mut rows = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let line_no = i + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if line_no == 1 && trimmed.starts_with("input,") {
            continue;
        }
        let fields = parse_csv_line(line);
        assert_eq!(fields.len(), 5, "line {line_no} fields={}", fields.len());
        rows.push(Row {
            line_no,
            input: fields[0].clone(),
            expected: fields[1].clone(),
            variant: fields[2].clone(),
            category: fields[3].clone(),
        });
    }
    rows
}

fn parse_csv_line(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut iter = line.chars().peekable();
    while let Some(c) = iter.next() {
        match (c, in_quotes) {
            ('"', true) => {
                if iter.peek() == Some(&'"') {
                    field.push('"');
                    iter.next();
                } else {
                    in_quotes = false;
                }
            }
            ('"', false) => in_quotes = true,
            (',', false) => out.push(std::mem::take(&mut field)),
            (other, _) => field.push(other),
        }
    }
    out.push(field);
    out
}

fn rust_apply(variant: &str, input: &str) -> String {
    match variant {
        "base" => to_identity_match_form(input),
        "title" => to_identity_match_form_title(input),
        "punct" => to_identity_match_form_with_punctuation(input),
        "disamb" => to_identity_match_form_with_disambiguator_strip(input),
        other => panic!("unknown variant {other:?}"),
    }
}

fn pg_function(variant: &str) -> &'static str {
    match variant {
        "base" => "wxyc_identity_match_artist",
        "title" => "wxyc_identity_match_title",
        "punct" => "wxyc_identity_match_with_punctuation",
        "disamb" => "wxyc_identity_match_with_disambiguator_strip",
        other => panic!("unknown variant {other:?}"),
    }
}

/// Idempotent setup: extension, dictionary, then the function definitions.
/// Re-running drops the dictionary first so the latest rules file binds.
fn install_functions(client: &mut Client) {
    client
        .batch_execute("CREATE EXTENSION IF NOT EXISTS unaccent;")
        .expect("create extension unaccent — required by parity test");
    // Drop+recreate the dictionary so each run picks up rules-file edits.
    client
        .batch_execute(
            "DROP TEXT SEARCH DICTIONARY IF EXISTS wxyc_unaccent; \
             CREATE TEXT SEARCH DICTIONARY wxyc_unaccent ( \
               TEMPLATE = unaccent, RULES = 'wxyc_unaccent' \
             );",
        )
        .expect(
            "create wxyc_unaccent dictionary — install \
             data/wxyc_unaccent.rules into $SHAREDIR/tsearch_data first; \
             see scripts/install_wxyc_unaccent.sh",
        );
    let sql = fs::read_to_string(functions_sql_path()).expect("read functions SQL");
    client
        .batch_execute(&sql)
        .expect("install wxyc_identity_match_* functions");
}

#[test]
#[ignore]
fn postgres_parity_matches_rust_for_every_fixture_row() {
    let Some(db_url) = test_db_url() else {
        eprintln!("TEST_DATABASE_URL unset — skipping postgres parity test");
        return;
    };
    let mut client = Client::connect(&db_url, postgres::NoTls).expect("connect to test PG");
    install_functions(&mut client);

    let rows = parse_fixture();
    assert!(
        rows.len() >= 250,
        "fixture has {} rows, expected ≥250",
        rows.len()
    );

    let mut failures: Vec<String> = Vec::new();
    for row in &rows {
        // Sanity: the committed expected matches the Rust function output.
        // (Already covered by identity_normalization_parity, but cheap to repeat
        // here so PG failures are not confused with Rust drift.)
        let rust_out = rust_apply(&row.variant, &row.input);
        if rust_out != row.expected {
            failures.push(format!(
                "  line {} RUST drift [{}/{}] input={:?}\n    fixture={:?}\n    rust={:?}",
                row.line_no, row.variant, row.category, row.input, row.expected, rust_out
            ));
            continue;
        }
        let fn_name = pg_function(&row.variant);
        let pg_row = client
            .query_one(&format!("SELECT {fn_name}($1)"), &[&row.input])
            .unwrap_or_else(|e| {
                panic!(
                    "PG query failed line {} ({fn_name}, input={:?}): {e:?}",
                    row.line_no, row.input
                )
            });
        let pg_out: Option<String> = pg_row.get(0);
        let pg_out = pg_out.unwrap_or_default();
        if pg_out != row.expected {
            failures.push(format!(
                "  line {} PG drift [{}/{}] input={:?}\n    rust={:?}\n      pg={:?}",
                row.line_no, row.variant, row.category, row.input, rust_out, pg_out
            ));
        }
    }
    if !failures.is_empty() {
        panic!(
            "{} of {} parity rows failed:\n{}",
            failures.len(),
            rows.len(),
            failures.join("\n")
        );
    }
}

#[test]
#[ignore]
fn postgres_parity_covers_all_four_entry_points() {
    let Some(db_url) = test_db_url() else {
        eprintln!("TEST_DATABASE_URL unset — skipping postgres parity test");
        return;
    };
    let mut client = Client::connect(&db_url, postgres::NoTls).expect("connect to test PG");
    install_functions(&mut client);

    for fn_name in [
        "wxyc_identity_match_artist",
        "wxyc_identity_match_title",
        "wxyc_identity_match_with_punctuation",
        "wxyc_identity_match_with_disambiguator_strip",
    ] {
        let row = client
            .query_one(&format!("SELECT {fn_name}('Stereolab')"), &[])
            .unwrap_or_else(|e| panic!("{fn_name} smoke failed: {e:?}"));
        let got: Option<String> = row.get(0);
        assert_eq!(got.as_deref(), Some("stereolab"), "{fn_name} smoke");

        // Idempotence: each function applied to its own output must produce
        // the same result. Mirrors the Rust-side idempotence assertions in
        // `wxyc_etl::text::identity::tests`. Uses a composed input so the
        // article-drop + paren-strip layers are exercised.
        let probe = "   The Foo Fighters (1995)   ";
        let once_row = client
            .query_one(&format!("SELECT {fn_name}($1)"), &[&probe])
            .unwrap_or_else(|e| panic!("{fn_name} idempotence-1 failed: {e:?}"));
        let once: Option<String> = once_row.get(0);
        let once = once.expect("non-null output");
        let twice_row = client
            .query_one(&format!("SELECT {fn_name}($1)"), &[&once])
            .unwrap_or_else(|e| panic!("{fn_name} idempotence-2 failed: {e:?}"));
        let twice: Option<String> = twice_row.get(0);
        assert_eq!(
            twice.as_deref(),
            Some(once.as_str()),
            "{fn_name} not idempotent: once={once:?} twice={twice:?}"
        );
    }
}
