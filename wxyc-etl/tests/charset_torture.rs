//! WX-1.2.7 detector for the text-normalization layer.
//!
//! Drives the cross-repo `@wxyc/shared` charset-torture corpus through
//! `wxyc_etl::text::normalize_artist_name` and compares the output against
//! the corpus's `expected_match_form`. Failures known to require WX-2
//! normalizer work are recorded in `EXPECTED_FAILURES` with a stable
//! `[etl:<reason>]` tag (the cross-reference key WX-3 / WX-2 auditors use).
//!
//! Two failure shapes both fail the test:
//!  - Unexpected failure: an entry mismatched its expected form and is not in
//!    EXPECTED_FAILURES. A genuine new bug or a corpus drift.
//!  - Unexpected pass: an entry in EXPECTED_FAILURES now matches. A fix
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

/// Inputs whose `normalize_artist_name(input) != expected_match_form` today.
/// Each value is the WX-1 plan tag explaining why; tags are searchable so
/// WX-2/WX-3 auditors can convert them to fixed boundaries.
fn expected_failures() -> HashMap<&'static str, &'static str> {
    let mut m = HashMap::new();
    // mojibake_known: requires WX-2 `to_storage_form` repair before normalization.
    m.insert(
        "Î£tella",
        "[etl:no-storage-form] needs WX-2 repair: Î£ -> Σ before normalize",
    );
    m.insert("Ã©", "[etl:no-storage-form] needs WX-2 repair: Ã© -> é");
    m.insert("Ã±", "[etl:no-storage-form] needs WX-2 repair: Ã± -> ñ");
    m.insert("Ã¶", "[etl:no-storage-form] needs WX-2 repair: Ã¶ -> ö");
    m.insert("Ã¼", "[etl:no-storage-form] needs WX-2 repair: Ã¼ -> ü");
    m.insert("â€™", "[etl:no-storage-form] needs WX-2 repair: â€™ -> ’");
    // Cyrillic Ё (U+0401) NFKD-decomposes to Е + combining diaeresis; the
    // diaeresis-stripping pass collapses it to "е", losing the dot. The corpus
    // expects "ё" preserved. WX-2 normalizer charter has to decide whether
    // script-essential marks survive — Ё, Й, etc. behave differently from
    // Latin é, ñ which are diacritic-as-decoration.
    m.insert(
        "Ё",
        "[etl:no-match-form] WX-2 charter must preserve Cyrillic Ё diaeresis",
    );
    // bidi_marks: Cf format chars (LRM/RLM/RLO/PDF) survive NFKD + is_mark
    // filter because they are category Cf, not M. The WX-2 charter strips
    // all Cf in match-form except U+200D (ZWJ). normalize_artist_name does
    // not yet implement that strip — it will when M2.2.5 deprecates the
    // legacy normalizer in favor of to_match_form.
    m.insert(
        "Hello\u{200E}World",
        "[etl:no-match-form] WX-2 strips Cf in to_match_form (not yet wired into normalize_artist_name)",
    );
    m.insert(
        "Hello\u{200F}World",
        "[etl:no-match-form] WX-2 strips Cf in to_match_form (not yet wired into normalize_artist_name)",
    );
    m.insert(
        "\u{202E}Reversed\u{202C}",
        "[etl:no-match-form] WX-2 strips Cf in to_match_form (not yet wired into normalize_artist_name)",
    );
    m
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
            let actual = wxyc_etl::text::normalize_artist_name(&entry.input);
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
