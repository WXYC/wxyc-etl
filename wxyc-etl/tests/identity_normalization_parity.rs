//! Parity matrix for the cross-cache-identity layered normalizer.
//!
//! Loads `tests/fixtures/identity_normalization_cases.csv` (see plan §3.3.3
//! parity-test matrix) and asserts the four identity functions reproduce the
//! pinned expected outputs row-by-row. The CSV is the locked spec for this
//! layer; the Postgres analog (`wxyc_identity_match_artist`) targets the
//! same outputs byte-for-byte.
//!
//! Format:
//!   input,expected,variant,category,notes
//!   variant ∈ {base, title, punct, disamb, article}
//!
//! Lines starting with `#` are comments. Empty lines are skipped. Quoted
//! fields use the same minimal CSV convention as the WX-1 charset-torture
//! corpus: a field starts with `"` and ends with `"`; embedded `""` is a
//! literal quote. No multi-line records.
//!
//! Steps 1-3 + 7 (NFKC, lowercase, combining-strip, sigma/folds, ASCII space
//! collapse) are not exercised here — that scope belongs to the WX-1
//! charset-torture corpus. Rows in this matrix vary along the §3.3.2
//! step-4 / 5 / 6 / 8 axes.

use std::fs;
use std::path::PathBuf;

use wxyc_etl::text::{
    strip_leading_article, to_identity_match_form, to_identity_match_form_title,
    to_identity_match_form_with_disambiguator_strip, to_identity_match_form_with_punctuation,
};

#[derive(Debug)]
struct Row {
    line_no: usize,
    input: String,
    expected: String,
    variant: String,
    category: String,
    notes: String,
}

fn fixture_path() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push("tests/fixtures/identity_normalization_cases.csv");
    p
}

fn parse_fixture() -> Vec<Row> {
    let text = fs::read_to_string(fixture_path()).expect("fixture file is missing");
    let mut rows = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let line_no = i + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        // Skip the header.
        if line_no == 1 && trimmed.starts_with("input,") {
            continue;
        }
        let fields = parse_csv_line(line);
        assert_eq!(
            fields.len(),
            5,
            "line {line_no} has {} fields, expected 5: {line:?}",
            fields.len()
        );
        rows.push(Row {
            line_no,
            input: fields[0].clone(),
            expected: fields[1].clone(),
            variant: fields[2].clone(),
            category: fields[3].clone(),
            notes: fields[4].clone(),
        });
    }
    rows
}

/// Minimal CSV parser. Handles quoted fields with `""` for embedded quotes.
/// Ignores newlines inside quotes (we do not produce multi-line records).
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
            ('"', false) => {
                in_quotes = true;
            }
            (',', false) => {
                out.push(std::mem::take(&mut field));
            }
            (other, _) => {
                field.push(other);
            }
        }
    }
    out.push(field);
    out
}

fn apply(variant: &str, input: &str) -> String {
    match variant {
        "base" => to_identity_match_form(input),
        "title" => to_identity_match_form_title(input),
        "punct" => to_identity_match_form_with_punctuation(input),
        "disamb" => to_identity_match_form_with_disambiguator_strip(input),
        "article" => strip_leading_article(input).to_string(),
        other => panic!("unknown variant {other:?}"),
    }
}

#[test]
fn identity_normalization_parity_matrix() {
    let rows = parse_fixture();
    assert!(
        rows.len() >= 250,
        "fixture must have ≥250 rows per plan §3.3.3, found {}",
        rows.len()
    );
    let mut failures: Vec<String> = Vec::new();
    for row in &rows {
        let got = apply(&row.variant, &row.input);
        if got != row.expected {
            failures.push(format!(
                "  line {} [{}/{}] input={:?}\n    expected={:?}\n         got={:?}\n    notes={:?}",
                row.line_no, row.variant, row.category, row.input, row.expected, got, row.notes
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
fn fixture_meets_categorical_coverage_minimums() {
    let rows = parse_fixture();
    let mut counts = std::collections::HashMap::<String, usize>::new();
    for row in &rows {
        *counts.entry(row.category.clone()).or_default() += 1;
    }
    // Per plan §3.3.3 categorical coverage minimums.
    let required: &[(&str, usize)] = &[
        ("trailing-parens-artist", 50),
        ("trailing-parens-title", 50),
        ("leading-article", 25),
        ("leading-article-strip", 10),
        ("punct", 50),
        ("disamb", 50),
        ("disamb-title-negative", 25),
    ];
    let mut shortfalls = Vec::new();
    for (cat, min) in required {
        let got = counts.get(*cat).copied().unwrap_or(0);
        if got < *min {
            shortfalls.push(format!("  {cat}: got {got}, need ≥{min}"));
        }
    }
    if !shortfalls.is_empty() {
        panic!(
            "categorical coverage shortfalls:\n{}",
            shortfalls.join("\n")
        );
    }
}
