//! WX-2.2.2 acceptance test for `to_match_form`.
//!
//! Drives every entry of the WX-1 charset-torture corpus that defines an
//! `expected_match_form` through `wxyc_etl::text::to_match_form` and asserts
//! the result equals it. Companion to `tests/forms_storage.rs` and the
//! legacy `tests/charset_torture.rs` (which still exercises the
//! pre-charter `normalize_artist_name` and carries `[etl:no-match-form]`
//! xfails until M2.2.5 deprecates that path).

use std::collections::HashMap;
use std::path::Path;

use serde::Deserialize;

#[derive(Deserialize)]
struct CorpusEntry {
    input: String,
    expected_match_form: Option<String>,
    notes: String,
}

#[derive(Deserialize)]
struct Corpus {
    categories: HashMap<String, Vec<CorpusEntry>>,
}

fn load_corpus() -> Corpus {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("tests/fixtures/charset-torture.json");
    let bytes = std::fs::read(&path).expect("vendored corpus exists");
    serde_json::from_slice(&bytes).expect("corpus is valid JSON")
}

#[test]
fn corpus_match_form_compliance() {
    let corpus = load_corpus();
    let mut failures: Vec<String> = Vec::new();

    for (category, entries) in &corpus.categories {
        for entry in entries {
            let Some(expected) = entry.expected_match_form.as_deref() else {
                continue;
            };
            let actual = wxyc_etl::text::to_match_form(&entry.input);
            if actual != expected {
                failures.push(format!(
                    "{category}: {input:?} -> {actual:?}, expected {expected:?}\n    notes: {notes}",
                    input = entry.input,
                    notes = entry.notes,
                ));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "\nto_match_form failures ({}):\n  {}\n",
        failures.len(),
        failures.join("\n  ")
    );
}
