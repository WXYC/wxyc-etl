//! WX-1.2.7 detector for the text-normalization layer.
//!
//! Drives the cross-repo `@wxyc/shared` charset-torture corpus through
//! `wxyc_etl::text::to_match_form` and compares the output against
//! the corpus's `expected_match_form`. Failures known to require future
//! charter work are recorded in `expected_failures()` with a stable
//! `[etl:<reason>]` tag (the cross-reference key WX-3 / WX-2 auditors use).
//!
//! Two failure shapes both fail the test:
//!  - Unexpected failure: an entry mismatched its expected form and is not in
//!    `expected_failures()`. A genuine new bug or a corpus drift.
//!  - Unexpected pass: an entry in `expected_failures()` now matches. A fix
//!    landed without updating the map; remove the entry.
//!
//! See WXYC/docs#15 for the WX-1 plan.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use serde::Deserialize;

#[derive(Deserialize, Debug)]
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

/// Inputs whose `to_match_form(input) != expected_match_form` today.
/// Each value is the WX-1 plan tag explaining why; tags are searchable so
/// WX-2/WX-3 auditors can convert them to fixed boundaries.
///
/// Empty since `to_match_form` covers every charter-form entry in the corpus
/// (the 9 prior `[etl:no-storage-form]` / `[etl:no-match-form]` entries
/// retired in WX-4.1.1 when the detector swapped off the legacy normalizer).
fn expected_failures() -> HashMap<&'static str, &'static str> {
    HashMap::new()
}

#[test]
fn corpus_match_form_compliance() {
    let corpus = load_corpus();
    let known_failures = expected_failures();

    let mut unexpected_failures: Vec<String> = Vec::new();
    let mut unexpected_passes: Vec<String> = Vec::new();
    let mut covered: HashSet<&str> = HashSet::new();

    for (category, entries) in &corpus.categories {
        for entry in entries {
            let Some(expected) = entry.expected_match_form.as_deref() else {
                continue;
            };
            let actual = wxyc_etl::text::to_match_form(&entry.input);
            let known = known_failures.get(entry.input.as_str()).copied();

            match (actual == expected, known) {
                (true, None) => {} // pass, not expected to fail — fine
                (true, Some(_tag)) => {
                    unexpected_passes.push(format!(
                        "{category}: {input:?} now matches {expected:?}; remove from EXPECTED_FAILURES",
                        input = entry.input
                    ));
                    covered.insert(entry.input.as_str());
                }
                (false, Some(_tag)) => {
                    covered.insert(entry.input.as_str());
                }
                (false, None) => {
                    unexpected_failures.push(format!(
                        "{category}: {input:?} -> {actual:?}, expected {expected:?}\n    notes: {notes}",
                        input = entry.input,
                        notes = entry.notes,
                    ));
                }
            }
        }
    }

    let stale: Vec<&str> = known_failures
        .keys()
        .filter(|k| !covered.contains(*k))
        .copied()
        .collect();

    let mut report = String::new();
    if !unexpected_failures.is_empty() {
        report.push_str(&format!(
            "\nUnexpected failures ({}):\n  {}\n",
            unexpected_failures.len(),
            unexpected_failures.join("\n  ")
        ));
    }
    if !unexpected_passes.is_empty() {
        report.push_str(&format!(
            "\nUnexpected passes ({}):\n  {}\n",
            unexpected_passes.len(),
            unexpected_passes.join("\n  ")
        ));
    }
    if !stale.is_empty() {
        report.push_str(&format!(
            "\nStale entries in EXPECTED_FAILURES (corpus no longer contains them): {stale:?}\n"
        ));
    }

    assert!(report.is_empty(), "{report}");
}
