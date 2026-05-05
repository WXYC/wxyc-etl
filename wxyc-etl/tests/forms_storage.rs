//! WX-2.2.1 acceptance test for `to_storage_form`.
//!
//! Drives every entry of the WX-1 charset-torture corpus through
//! `wxyc_etl::text::to_storage_form` and asserts the result equals the
//! corpus's `expected_storage`. Three classes of assertion:
//!
//! - `mojibake_known` (6 entries): the fix-pair is reversed.
//! - `normalization` NFD entries (2 entries): bytes canonicalize to NFC.
//! - everything else (~30 entries): storage form is a passthrough.
//!
//! Companion to `tests/charset_torture.rs` (which exercises the legacy
//! `normalize_artist_name` path).

use std::collections::HashMap;
use std::path::Path;

use serde::Deserialize;

#[derive(Deserialize)]
struct CorpusEntry {
    input: String,
    expected_storage: String,
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
fn corpus_storage_form_compliance() {
    let corpus = load_corpus();
    let mut failures: Vec<String> = Vec::new();

    for (category, entries) in &corpus.categories {
        for entry in entries {
            let actual = wxyc_etl::text::to_storage_form(&entry.input);
            if actual != entry.expected_storage {
                failures.push(format!(
                    "{category}: {input:?} -> {actual:?}, expected {expected:?}\n    notes: {notes}",
                    input = entry.input,
                    expected = entry.expected_storage,
                    notes = entry.notes,
                ));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "\nto_storage_form failures ({}):\n  {}\n",
        failures.len(),
        failures.join("\n  ")
    );
}
