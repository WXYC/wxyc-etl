//! WX-2.2.3 acceptance test for `to_ascii_form`.
//!
//! Drives every entry of the WX-1 charset-torture corpus that defines a
//! non-null `expected_ascii_form` through `wxyc_etl::text::to_ascii_form`
//! and asserts the result equals it. Companion to `tests/forms_storage.rs`
//! and `tests/forms_match.rs`.
//!
//! Entries with `expected_ascii_form: null` (CJK, Arabic, Hebrew, raw
//! emoji rendered as music symbols) are skipped — those scripts have no
//! committed v1 transliteration.

use std::collections::HashMap;
use std::path::Path;

use serde::Deserialize;

#[derive(Deserialize)]
struct CorpusEntry {
    input: String,
    expected_ascii_form: Option<String>,
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
fn corpus_ascii_form_compliance() {
    let corpus = load_corpus();
    let mut failures: Vec<String> = Vec::new();

    for (category, entries) in &corpus.categories {
        for entry in entries {
            let Some(expected) = entry.expected_ascii_form.as_deref() else {
                continue;
            };
            let actual = wxyc_etl::text::to_ascii_form(&entry.input);
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
        "\nto_ascii_form failures ({}):\n  {}\n",
        failures.len(),
        failures.join("\n  ")
    );
}
