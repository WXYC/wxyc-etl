//! Regression report for the cross-cache-identity layered normalizer.
//!
//! Spec: plan §3.3.4 (regression-report required artifact). Baselines
//! `to_identity_match_form` and the opt-in variants against `to_match_form`
//! (NOT the legacy `normalize_artist_name` — that delta is already captured
//! in WX-2.3 per-repo migration).
//!
//! ## Output
//!
//! `target/regression-report.json` — structured object with sections:
//!
//! - `A` per-row diff: rows where the identity form differs from the match
//!   form, with which §3.3.2 step (or composition of steps) caused each diff.
//! - `B` per-step impact: rows changed only by step 4 / 5 / 6 / 8 in isolation.
//! - `C` identity match shift: match LOST / GAINED / CHANGED bucket counts
//!   when distinct-`norm_artist` partitions are recomputed under the identity
//!   form vs. the match form. Computed from the input alone — no live PG join.
//! - `D` per-step match shift: the C analysis broken down per §3.3.2 step.
//! - `E` confidence-threshold validation: stub. Requires reading
//!   `entity.identity.confidence` from a live cache; the input CSV alone is
//!   not enough. Populated by a follow-up PR or by an inline run against
//!   prod-equivalent local Homebrew caches with PG access wired in.
//!
//! ## How to run
//!
//! ```bash
//! WXYC_REGRESSION_INPUT=/path/to/artists.csv cargo test --test regression_report -- --include-ignored
//! ```
//!
//! Input CSV: a `name` column of artist strings. Optional: `library_id`
//! column for per-row tagging in the report. Header row required.
//!
//! Without the env var the test is `#[ignore]`-marked and skipped.
//!
//! ## Acceptable thresholds (plan §3.3.4)
//!
//! - C aggregate ≤2%: green
//! - C 2-5%: project-lead review
//! - C >5%: algorithm refinement before ship
//! - D step 4 / 5: any per-step shift triggers algorithm refinement (locked-on)
//! - D step 6 / 8: shift >2% gates the step behind a separately-named opt-in
//!   function (already done — they ship as `_with_punctuation` and
//!   `_with_disambiguator_strip`)

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;

use serde::Serialize;

use wxyc_etl::text::{
    to_identity_match_form, to_identity_match_form_with_disambiguator_strip,
    to_identity_match_form_with_punctuation, to_match_form,
};

/// Per-step impact tag — which §3.3.2 steps changed the value relative to
/// `to_match_form`.
#[derive(Debug, Default, Serialize)]
struct StepFlags {
    /// Step 4: trailing paren strip.
    paren_strip: bool,
    /// Step 5: leading article drop (or Discogs comma form).
    article_drop: bool,
    /// Step 6: punctuation collapse (only via `_with_punctuation`).
    punct_collapse: bool,
    /// Step 8: trailing `/N` disambiguator strip (only via
    /// `_with_disambiguator_strip`).
    disambiguator_strip: bool,
}

/// One row of section A.
#[derive(Debug, Serialize)]
struct DiffRow {
    library_id: Option<String>,
    input: String,
    match_form: String,
    identity_form: String,
    identity_with_punctuation: String,
    identity_with_disambiguator_strip: String,
    steps_changed: StepFlags,
}

/// Aggregate counters for section B.
#[derive(Debug, Default, Serialize)]
struct StepImpact {
    /// Total rows whose identity form differs from match form.
    rows_changed_total: usize,
    /// Rows changed by exactly one step (the step indicated).
    rows_changed_only_by_paren_strip: usize,
    rows_changed_only_by_article_drop: usize,
    rows_changed_only_by_punct_collapse: usize,
    rows_changed_only_by_disambiguator_strip: usize,
    /// Rows where multiple steps fired.
    rows_changed_by_multiple_steps: usize,
}

/// Section C / D shift counters. We compute three buckets relative to the
/// input partition size (number of distinct `match_form` values):
///
/// - LOST: rows whose `match_form` value was unique under match form but
///   collides with another row's value under identity form (i.e. two
///   previously-distinct entities now share a key — match identity is *lost*
///   from the perspective of preserving distinctness).
/// - GAINED: rows whose `match_form` shared a key with another row but is
///   now unique under identity form (rare — typically zero, since identity
///   is strictly more aggressive).
/// - CHANGED: rows whose key changed but the partition assignment did not
///   (e.g. `the foo` → `foo`; if no other row was already `foo`, the row's
///   partition still has 1 member — but the key itself shifted).
#[derive(Debug, Default, Serialize)]
struct ShiftCounters {
    rows_total: usize,
    distinct_match_form_keys: usize,
    distinct_identity_form_keys: usize,
    rows_with_changed_key: usize,
    rows_lost_distinctness: usize,
    rows_gained_distinctness: usize,
    /// `rows_with_changed_key / rows_total` as a percentage with 2 decimals.
    pct_changed: f64,
    /// `rows_lost_distinctness / rows_total` as a percentage. The headline
    /// metric for the plan §3.3.4 ≤2% / 2-5% / >5% thresholds.
    pct_lost: f64,
}

#[derive(Debug, Default, Serialize)]
struct PerStepShift {
    step_4_paren_strip: ShiftCounters,
    step_5_article_drop: ShiftCounters,
    step_6_punct_collapse: ShiftCounters,
    step_8_disambiguator_strip: ShiftCounters,
}

#[derive(Debug, Serialize)]
struct Report {
    spec_reference: &'static str,
    plan_section: &'static str,
    baseline: &'static str,
    input_path: String,
    rows_total: usize,
    section_a_per_row_diff: Vec<DiffRow>,
    section_b_step_impact: StepImpact,
    section_c_identity_match_shift: ShiftCounters,
    section_d_per_step_shift: PerStepShift,
    section_e_confidence_threshold_validation: ConfidenceStub,
}

#[derive(Debug, Default, Serialize)]
struct ConfidenceStub {
    note: &'static str,
}

#[derive(Debug)]
struct InputRow {
    library_id: Option<String>,
    name: String,
}

fn read_input(path: &str) -> Vec<InputRow> {
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(true)
        .from_path(path)
        .expect("input CSV unreadable");
    let headers = rdr.headers().expect("CSV missing headers").clone();
    let name_idx = headers
        .iter()
        .position(|h| h == "name" || h == "artist_name")
        .expect("CSV missing 'name' or 'artist_name' header");
    let id_idx = headers.iter().position(|h| h == "library_id");
    let mut rows = Vec::new();
    for record in rdr.records() {
        let r = record.expect("malformed CSV row");
        let name = r.get(name_idx).map(|s| s.to_string()).unwrap_or_default();
        let library_id = id_idx.and_then(|i| r.get(i).map(|s| s.to_string()));
        rows.push(InputRow { library_id, name });
    }
    rows
}

/// Compute which §3.3.2 steps fired between `match_form` and the identity
/// forms. The function decides per-step from the relationship between the
/// match form, the base identity form, and the two opt-in forms — not from
/// a heuristic over `match_form` alone. This avoids the "step 6/8 always
/// reads 0" failure mode that caught a previous version of this code.
///
/// Rules:
/// - Step 4 (paren strip): fires iff `match_form` ends with `)` or `]` AND
///   the base identity differs (the strip ran).
/// - Step 5 (article drop): fires iff `match_form` has a leading `the / a /
///   an ` or trailing `, the / , a / , an` AND the base identity differs.
/// - Step 6 (punct collapse): fires iff `_with_punctuation` differs from
///   the base identity. This is the *definitive* signal — that variant's
///   only added behavior is the punct collapse.
/// - Step 8 (disambiguator strip): fires iff `_with_disambiguator_strip`
///   differs from the base identity. Same definitive-signal logic.
fn classify_steps(match_v: &str, identity_v: &str, punct_v: &str, disamb_v: &str) -> StepFlags {
    let mut flags = StepFlags::default();

    // Steps 4 + 5 are inside the base identity pipeline. They could only
    // have fired if the base differs from match form *and* the match form
    // had the corresponding signature.
    if match_v != identity_v {
        if (match_v.ends_with(')') && match_v.contains('('))
            || (match_v.ends_with(']') && match_v.contains('['))
        {
            flags.paren_strip = true;
        }
        if match_v.starts_with("the ")
            || match_v.starts_with("a ")
            || match_v.starts_with("an ")
            || match_v.ends_with(", the")
            || match_v.ends_with(", a")
            || match_v.ends_with(", an")
        {
            flags.article_drop = true;
        }
    }

    // Step 6: punctuation collapse is the *only* additional behavior in the
    // _with_punctuation variant relative to the base. Any divergence from
    // the base identity means this step fired.
    if punct_v != identity_v {
        flags.punct_collapse = true;
    }

    // Step 8: same logic — _with_disambiguator_strip's only addition over
    // the base is the trailing /N strip.
    if disamb_v != identity_v {
        flags.disambiguator_strip = true;
    }

    flags
}

fn compute_section_a(rows: &[InputRow]) -> Vec<DiffRow> {
    let mut out = Vec::new();
    for row in rows {
        let m = to_match_form(&row.name);
        let i_base = to_identity_match_form(&row.name);
        let i_punct = to_identity_match_form_with_punctuation(&row.name);
        let i_disamb = to_identity_match_form_with_disambiguator_strip(&row.name);
        if m == i_base && m == i_punct && m == i_disamb {
            continue;
        }
        let steps = classify_steps(&m, &i_base, &i_punct, &i_disamb);
        out.push(DiffRow {
            library_id: row.library_id.clone(),
            input: row.name.clone(),
            match_form: m,
            identity_form: i_base,
            identity_with_punctuation: i_punct,
            identity_with_disambiguator_strip: i_disamb,
            steps_changed: steps,
        });
    }
    out
}

fn compute_section_b(rows: &[DiffRow]) -> StepImpact {
    let mut out = StepImpact {
        rows_changed_total: rows.len(),
        ..Default::default()
    };
    for r in rows {
        let s = &r.steps_changed;
        let count = [
            s.paren_strip,
            s.article_drop,
            s.punct_collapse,
            s.disambiguator_strip,
        ]
        .iter()
        .filter(|b| **b)
        .count();
        match count {
            0 => {} // not changed; shouldn't appear in section A but be defensive
            1 => {
                if s.paren_strip {
                    out.rows_changed_only_by_paren_strip += 1;
                } else if s.article_drop {
                    out.rows_changed_only_by_article_drop += 1;
                } else if s.punct_collapse {
                    out.rows_changed_only_by_punct_collapse += 1;
                } else if s.disambiguator_strip {
                    out.rows_changed_only_by_disambiguator_strip += 1;
                }
            }
            _ => out.rows_changed_by_multiple_steps += 1,
        }
    }
    out
}

fn compute_shift<F>(rows: &[InputRow], to_form: F) -> ShiftCounters
where
    F: Fn(&str) -> String,
{
    let total = rows.len();
    let match_keys: Vec<String> = rows.iter().map(|r| to_match_form(&r.name)).collect();
    let identity_keys: Vec<String> = rows.iter().map(|r| to_form(&r.name)).collect();

    let distinct_match_form: HashSet<&String> = match_keys.iter().collect();
    let distinct_identity_form: HashSet<&String> = identity_keys.iter().collect();

    let mut match_partition: HashMap<&String, Vec<usize>> = HashMap::new();
    let mut identity_partition: HashMap<&String, Vec<usize>> = HashMap::new();
    for (i, k) in match_keys.iter().enumerate() {
        match_partition.entry(k).or_default().push(i);
    }
    for (i, k) in identity_keys.iter().enumerate() {
        identity_partition.entry(k).or_default().push(i);
    }

    let mut rows_with_changed_key = 0;
    let mut rows_lost_distinctness = 0;
    let mut rows_gained_distinctness = 0;
    for i in 0..total {
        let was_unique = match_partition[&match_keys[i]].len() == 1;
        let is_unique = identity_partition[&identity_keys[i]].len() == 1;
        if match_keys[i] != identity_keys[i] {
            rows_with_changed_key += 1;
        }
        if was_unique && !is_unique {
            rows_lost_distinctness += 1;
        }
        if !was_unique && is_unique {
            rows_gained_distinctness += 1;
        }
    }

    let pct = |n: usize| -> f64 {
        if total == 0 {
            0.0
        } else {
            ((n as f64 / total as f64) * 10000.0).round() / 100.0
        }
    };
    ShiftCounters {
        rows_total: total,
        distinct_match_form_keys: distinct_match_form.len(),
        distinct_identity_form_keys: distinct_identity_form.len(),
        rows_with_changed_key,
        rows_lost_distinctness,
        rows_gained_distinctness,
        pct_changed: pct(rows_with_changed_key),
        pct_lost: pct(rows_lost_distinctness),
    }
}

fn write_report(report: &Report) {
    let mut out = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    out.pop(); // workspace root
    out.push("target");
    fs::create_dir_all(&out).expect("target dir");
    out.push("regression-report.json");
    let json = serde_json::to_string_pretty(report).expect("serialize");
    fs::write(&out, json).expect("write report");
    eprintln!("regression-report written to {}", out.display());
    eprintln!(
        "  C aggregate (% rows lost distinctness): {:.2}%",
        report.section_c_identity_match_shift.pct_lost
    );
}

#[test]
#[ignore = "requires WXYC_REGRESSION_INPUT pointing at a CSV of artist names"]
fn regression_report() {
    let input_path = match std::env::var("WXYC_REGRESSION_INPUT") {
        Ok(v) => v,
        Err(_) => {
            eprintln!("WXYC_REGRESSION_INPUT unset; skipping");
            return;
        }
    };
    let rows = read_input(&input_path);
    let section_a = compute_section_a(&rows);
    let section_b = compute_section_b(&section_a);
    let section_c = compute_shift(&rows, to_identity_match_form);
    let section_d = PerStepShift {
        // Steps 4 + 5 are inseparable in the locked-on baseline, so the
        // step-4 and step-5 entries report the same composite shift. The
        // opt-in variants give clean per-step shifts for steps 6 and 8.
        step_4_paren_strip: compute_shift(&rows, to_identity_match_form),
        step_5_article_drop: compute_shift(&rows, to_identity_match_form),
        step_6_punct_collapse: compute_shift(&rows, to_identity_match_form_with_punctuation),
        step_8_disambiguator_strip: compute_shift(
            &rows,
            to_identity_match_form_with_disambiguator_strip,
        ),
    };
    let report = Report {
        spec_reference: "docs/normalization.md",
        plan_section: "library-hook-canonicalization-plan §3.3.4",
        baseline: "to_match_form",
        input_path,
        rows_total: rows.len(),
        section_a_per_row_diff: section_a,
        section_b_step_impact: section_b,
        section_c_identity_match_shift: section_c,
        section_d_per_step_shift: section_d,
        section_e_confidence_threshold_validation: ConfidenceStub {
            note: "Section E requires entity.identity.confidence values from a live cache; the input CSV alone is insufficient. Populate by joining this report's section-A library_id list against your local Homebrew Discogs/MusicBrainz/Wikidata caches and recomputing confidence-band membership under to_identity_match_form. See plan §3.3.4 for thresholds.",
        },
    };
    write_report(&report);
}

// --- self-test against the parity matrix CSV (always runs, no env var) ---

#[test]
fn self_test_runs_against_parity_matrix() {
    // Use the categorical parity matrix as a synthetic input. Verifies the
    // harness produces a well-formed JSON report and the section-C/D math
    // doesn't blow up on edge cases.
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("tests/fixtures/identity_normalization_cases.csv");
    let text = fs::read_to_string(&path).expect("fixture readable");
    let mut input_rows = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if i == 0 && trimmed.starts_with("input,") {
            continue;
        }
        // The parity-matrix CSV's first column is `input`, which is the
        // artist name we want to feed to the report.
        let name = parse_first_field(line);
        input_rows.push(InputRow {
            library_id: None,
            name,
        });
    }
    assert!(input_rows.len() >= 250, "fixture too small");

    let section_a = compute_section_a(&input_rows);
    let section_b = compute_section_b(&section_a);
    let section_c = compute_shift(&input_rows, to_identity_match_form);

    // Sanity: at least some rows differ from match form (the fixture is
    // designed to exercise §3.3.2 steps 4-6 + 8).
    assert!(
        !section_a.is_empty(),
        "section A empty — parity matrix should exercise identity-specific steps"
    );
    assert_eq!(
        section_b.rows_changed_total,
        section_a.len(),
        "section B total must match section A length"
    );
    assert_eq!(section_c.rows_total, input_rows.len());
    assert!(section_c.distinct_match_form_keys >= 1);
    assert!(section_c.distinct_identity_form_keys >= 1);
    // Identity form should never produce *more* distinct keys than match form
    // (it is strictly more aggressive).
    assert!(
        section_c.distinct_identity_form_keys <= section_c.distinct_match_form_keys,
        "identity form produced {} distinct keys vs {} from match form — identity is supposed to be more aggressive",
        section_c.distinct_identity_form_keys,
        section_c.distinct_match_form_keys,
    );

    // Smoke-test the JSON serialization end-to-end so a future schema break
    // surfaces here, not at runtime against real data.
    let report = Report {
        spec_reference: "docs/normalization.md",
        plan_section: "library-hook-canonicalization-plan §3.3.4",
        baseline: "to_match_form",
        input_path: "self-test:identity_normalization_cases.csv".to_string(),
        rows_total: input_rows.len(),
        section_a_per_row_diff: section_a,
        section_b_step_impact: section_b,
        section_c_identity_match_shift: section_c,
        section_d_per_step_shift: PerStepShift::default(),
        section_e_confidence_threshold_validation: ConfidenceStub {
            note: "self-test does not populate section E",
        },
    };
    let json = serde_json::to_string(&report).expect("serialize");
    assert!(json.contains("section_a_per_row_diff"));
}

/// Regression test: classify_steps must set step-6 / step-8 flags whenever
/// the corresponding opt-in variant diverges from the base identity form. A
/// previous version short-circuited when `match_v == identity_v`, which
/// silently zeroed step-6 and step-8 attribution. This test pins the fix
/// at the per-row flag layer (section A); the aggregate section-B counters
/// derive from there.
///
/// Note on cross-step interaction: step 6 (punct collapse) operates on the
/// `/` in `John Smith /1`, so a row that triggers step 8 *also* triggers
/// step 6. There is no "pure step-8 only" input; the test asserts both
/// flags fire on `John Smith /1` rather than asserting an "only-by-disamb"
/// section-B counter.
#[test]
fn classify_steps_attributes_opt_in_variants() {
    // Pure punct case — `M.I.A.` has no parens, no article, no /N. Only
    // step 6 fires; step 8 does not (no trailing whitespace + slash).
    let pure_punct = InputRow {
        library_id: None,
        name: "M.I.A.".to_string(),
    };
    let section_a = compute_section_a(&[pure_punct]);
    assert_eq!(section_a.len(), 1);
    let row = &section_a[0];
    assert!(row.steps_changed.punct_collapse, "punct must be attributed");
    assert!(
        !row.steps_changed.disambiguator_strip,
        "disamb must NOT be attributed for input without trailing /N"
    );

    // Step-8 case — `John Smith /1`. Step 8 fires (disamb strips /1), and
    // step 6 also fires (punct collapses `/` to space).
    let with_disamb = InputRow {
        library_id: None,
        name: "John Smith /1".to_string(),
    };
    let section_a = compute_section_a(&[with_disamb]);
    assert_eq!(section_a.len(), 1);
    let row = &section_a[0];
    assert!(
        row.steps_changed.disambiguator_strip,
        "disamb must be attributed for trailing /N"
    );
    assert!(
        row.steps_changed.punct_collapse,
        "punct also fires because step 6 collapses the `/` character"
    );
}

fn parse_first_field(line: &str) -> String {
    // Mirrors the parity test's parser, just for the first field.
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
            (',', false) => break,
            (other, _) => field.push(other),
        }
    }
    field
}
