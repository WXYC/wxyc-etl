//! Generator + freshness check for `data/wxyc_unaccent.rules`.
//!
//! The rules file is the Postgres-side counterpart to the Rust
//! `strip_combining_selective` + `apply_folds` passes inside
//! [`to_match_form`]. It's vendored verbatim into every cache repo that
//! deploys the identity-match plpgsql functions; downstream parity tests
//! pin its SHA-256.
//!
//! Generation rule: for every codepoint `c` in the Latin / Greek decoration
//! scripts that [`to_match_form`] strips marks from, if
//! `to_match_form(c) != c` and the result is non-empty, single-line, and
//! contains no whitespace, emit `c → to_match_form(c)` as an unaccent rule.
//!
//! By construction, the resulting Postgres pipeline
//!
//! ```text
//! normalize(s, NFKC) → lower(s) → unaccent('wxyc_unaccent', s) → strip-Cf → collapse-space
//! ```
//!
//! produces byte-identical output to [`to_match_form`] for every input in
//! these script ranges. Inputs outside the ranges (Cyrillic Ё, Polish ł,
//! Turkish ı, Eszett ß) pass through unchanged on both sides.
//!
//! Capitals are not emitted: Postgres applies `lower()` before `unaccent()`,
//! so the rules only need lowercase keys. Greek final sigma `ς` and the
//! ligatures `æ`, `œ` are lowercase, so they fall out of the same scan.
//!
//! Default mode: assert the committed `data/wxyc_unaccent.rules` matches
//! the generator output exactly. Run with `WXYC_REGENERATE_RULES=1` to
//! overwrite the file with the regenerated content.

use std::fs;
use std::path::PathBuf;

use wxyc_etl::text::to_match_form;

const RULES_VERSION: &str = "0.1.0";

/// Codepoint ranges where Rust strips Latin / Greek combining marks
/// (`is_diacritic_decoration_script` in `text::forms`). Postgres mirrors
/// this scope through the rules file.
const RANGES: &[(u32, u32)] = &[
    (0x00A0, 0x024F), // Latin-1 Supplement + Latin Extended-A + Latin Extended-B
    (0x1E00, 0x1EFF), // Latin Extended Additional
    (0x2C60, 0x2C7F), // Latin Extended-C
    (0xA720, 0xA7FF), // Latin Extended-D
    (0xAB30, 0xAB6F), // Latin Extended-E
    (0x0370, 0x03FF), // Greek and Coptic
    (0x1F00, 0x1FFF), // Greek Extended
];

fn workspace_root() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop();
    p
}

fn rules_path() -> PathBuf {
    let mut p = workspace_root();
    p.push("data/wxyc_unaccent.rules");
    p
}

fn version_path() -> PathBuf {
    let mut p = workspace_root();
    p.push("data/wxyc_unaccent.version");
    p
}

/// Generate the rules content. No comments (Postgres `unaccent` does not
/// support `#` comment lines — it parses every line as a rule). Version
/// metadata lives in the sibling `wxyc_unaccent.version` file.
///
/// Skipping rule: emit only chars `c` where `lower(c) == c`. Postgres
/// applies `lower()` before `unaccent()`, so any char that lowercases to
/// something else is already transformed away before the dictionary
/// matches. This excludes the obvious `Σ`/`É`/`À` capitals but also Greek
/// titlecase like `ᾈ` (Unicode category `Lt` — `is_uppercase()` returns
/// false but `to_lowercase()` still changes them).
fn generate_rules() -> String {
    let mut entries: Vec<(u32, String, String)> = Vec::new();
    for &(start, end) in RANGES {
        for cp in start..=end {
            let Some(c) = char::from_u32(cp) else {
                continue;
            };
            let input = c.to_string();
            let lowered: String = c.to_lowercase().collect();
            if lowered != input {
                continue;
            }
            let output = to_match_form(&input);
            if output == input || output.is_empty() {
                continue;
            }
            if output.chars().any(|ch| ch.is_whitespace()) {
                continue;
            }
            entries.push((cp, input, output));
        }
    }
    entries.sort_by_key(|(cp, _, _)| *cp);

    let mut s = String::new();
    for (_, input, output) in &entries {
        s.push_str(&format!("{input}\t{output}\n"));
    }
    s
}

fn generate_version() -> String {
    format!("{RULES_VERSION}\n")
}

#[test]
fn rules_file_matches_generator() {
    let expected_rules = generate_rules();
    let expected_version = generate_version();
    let rules = rules_path();
    let version = version_path();

    // Require an explicit non-empty value so an accidentally-exported
    // empty env var doesn't silently rewrite the committed files.
    if std::env::var("WXYC_REGENERATE_RULES")
        .ok()
        .filter(|v| !v.is_empty())
        .is_some()
    {
        fs::write(&rules, &expected_rules).expect("write rules file");
        fs::write(&version, &expected_version).expect("write version file");
        eprintln!("regenerated {} + {}", rules.display(), version.display());
        return;
    }

    let actual_rules =
        fs::read_to_string(&rules).unwrap_or_else(|e| panic!("read {}: {e}", rules.display()));
    if actual_rules != expected_rules {
        panic!(
            "data/wxyc_unaccent.rules is stale.\n\
             Re-run with WXYC_REGENERATE_RULES=1 to refresh:\n  \
             WXYC_REGENERATE_RULES=1 cargo test --test wxyc_unaccent_rules_test\n\n\
             First divergence preview:\n{}",
            preview_diff(&actual_rules, &expected_rules)
        );
    }
    let actual_version =
        fs::read_to_string(&version).unwrap_or_else(|e| panic!("read {}: {e}", version.display()));
    assert_eq!(actual_version, expected_version, "version file drift");
}

#[test]
fn version_constant_is_semver_shaped() {
    let v = RULES_VERSION;
    assert_eq!(
        v.split('.').count(),
        3,
        "RULES_VERSION must be MAJOR.MINOR.PATCH, got {v:?}"
    );
}

fn preview_diff(actual: &str, expected: &str) -> String {
    let a: Vec<&str> = actual.lines().collect();
    let e: Vec<&str> = expected.lines().collect();
    let mut out = String::new();
    let max = a.len().max(e.len());
    let mut shown = 0;
    for i in 0..max {
        let al = a.get(i).copied().unwrap_or("<missing>");
        let el = e.get(i).copied().unwrap_or("<missing>");
        if al != el {
            out.push_str(&format!(
                "  line {i}:\n    on disk: {al}\n    expected: {el}\n"
            ));
            shown += 1;
            if shown >= 5 {
                out.push_str("  ... (further divergences elided)\n");
                break;
            }
        }
    }
    if out.is_empty() {
        out.push_str("  (only header / trailing whitespace differs)\n");
    }
    out
}
