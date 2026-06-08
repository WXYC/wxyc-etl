# Normalization spec (cross-cache-identity)

This document specifies the normalization functions used by the cross-cache-identity workstream — epics E2 (Backend identity record) and E3 (canonical normalization function) of [`library-hook-canonicalization-plan`](https://github.com/WXYC/wiki/blob/main/plans/library-hook-canonicalization-plan.md). It is the canonical reference for which function to call when implementing identity matching across the four caches (Backend, Discogs, MusicBrainz, Wikidata) and the LML reconciler.

For the underlying three-entry-point contracts (`to_storage_form`, `to_match_form`, `to_ascii_form`), the canonical authority is the **WX-2 Normalizer Charter** ([WXYC/docs#16](https://github.com/WXYC/docs/issues/16)). This document specifies the cross-cache-identity-specific layer that sits on top of WX-2 — the four extra steps from plan [§3.3.2](https://github.com/WXYC/wiki/blob/main/plans/library-hook-canonicalization-plan.md#332-algorithm-locked-except-where-noted) that `to_match_form` does not implement.

## Status

| Layer | State |
|---|---|
| WX-2 baseline (`to_storage_form` / `to_match_form` / `to_ascii_form`) | ✅ SHIPPED in `wxyc-etl` 0.2.x (commit `c2652b9`); legacy `normalize_artist_name` / `strip_diacritics` / `normalize_title` / `batch_normalize` removed in 0.7.0 (WX-4.1.1) |
| Audit of all consumer normalizers (plan [§3.3.1](https://github.com/WXYC/wiki/blob/main/plans/library-hook-canonicalization-plan.md#331-sequenced-workflow-with-explicit-decision-gates) step 1) | ✅ DONE in [`docs/normalization-audit.md`](./normalization-audit.md) (PR [#88](https://github.com/WXYC/wxyc-etl/pull/88), closed [#74](https://github.com/WXYC/wxyc-etl/issues/74)) |
| Cross-cache-identity layer (this doc) | 🚧 SPEC — implementation not yet shipped |
| Postgres analog | ⏳ FUTURE — see [Postgres analog](#postgres-analog-wxyc_identity_match_artist) |

## Canonical entry points

| Function | Layer | Use it when… | Reference |
|---|---|---|---|
| `to_storage_form(s)` | WX-2 | Storing user-input bytes (NFC + mojibake fix; preserves human-readable form) | docs#16 |
| `to_match_form(s)` | WX-2 | Comparing two strings for equivalence in search/FTS5/prefix lookup (NFKC + case-fold + strip combining marks + WXYC script folds) | docs#16 |
| `to_ascii_form(s)` | WX-2 | Last-resort ASCII fallback after `to_match_form` returned no candidates (transliterates via `deunicode`) | docs#16 |
| **`to_identity_match_form(s)`** | **identity (this doc)** | **Comparing two strings for cross-cache identity matching — i.e., resolving `library_id ↔ discogs_master_id ↔ MBID ↔ Wikidata Q-id`** | This doc, plan [§3.3.2](https://github.com/WXYC/wiki/blob/main/plans/library-hook-canonicalization-plan.md#332-algorithm-locked-except-where-noted) steps 4-6 + 8 |

`to_identity_match_form` is **strictly more aggressive** than `to_match_form`: anything `to_match_form` collapses, `to_identity_match_form` also collapses. The reverse is not true — identity matching deliberately drops "Remastered 2019" parens, leading articles, internal punctuation, and trailing `/N` disambiguators that `to_match_form` preserves for FTS5 / prefix-lookup users.

## The identity-specific delta

`to_match_form` covers plan [§3.3.2](https://github.com/WXYC/wiki/blob/main/plans/library-hook-canonicalization-plan.md#332-algorithm-locked-except-where-noted) steps 1-3 + 7 (NFKD/NFKC, drop combining marks, lowercase, collapse whitespace) plus mojibake/sigma/folds inherited from WX-2's `to_storage_form`. The four steps `to_match_form` does **not** implement are:

| Step | Behavior | Locked-on or opt-in? |
|---|---|---|
| 4. Paren strip at end | `Foo (Remastered 2019)` → `foo` | **Locked-on** — fixes obvious LML reconciliation errors; non-negotiable per plan [§3.3.4](https://github.com/WXYC/wiki/blob/main/plans/library-hook-canonicalization-plan.md#334-regression-report-required-artifact) |
| 5. Leading article drop | `The Beatles` → `beatles`; `An Albatross` → `albatross` | **Locked-on** — same rationale |
| 6. Punctuation-to-space collapse | `Godspeed You! Black Emperor` → `godspeed you black emperor` | **Opt-in candidate** — ships if regression report's per-step shift on this rule is ≤2%; otherwise gated behind `to_identity_match_form_with_punctuation` (separately-named function per plan [§3.3.4](https://github.com/WXYC/wiki/blob/main/plans/library-hook-canonicalization-plan.md#334-regression-report-required-artifact)) |
| 8. Trailing `/N` disambiguator strip (artists only) | `John Smith /1` → `john smith` | **Opt-in candidate** — same gating as step 6; never applies to titles (Side A/B style is meaningful) |

Steps 4 + 5 land first; the regression report determines whether 6 + 8 ship as locked-on or as separately-named opt-in functions.

## Architectural decision: `to_identity_match_form` (option 1 per plan §3.3.0)

The plan's [§3.3.0](https://github.com/WXYC/wiki/blob/main/plans/library-hook-canonicalization-plan.md#330-status-update-2026-05-06-wx-2-normalizer-charter-has-reshaped-this-section) lists three architectural options:
1. New layered function in `wxyc-etl` calling `to_match_form` and adding the four extra steps
2. Extend `to_match_form` itself
3. Ship as a separate function inside Backend-Service or LML

**Decision: option 1.** Rationale:

- Option 2 would couple every WX-2.3.x consumer (LML's FTS5 fallback, semantic-index artist resolver, etc.) to identity-matching semantics they don't want. FTS5 specifically should preserve `(Live)` / `(Remastered)` so users can find live-album-specific entries.
- Option 3 reintroduces the per-repo divergence the WX-2 charter just fixed. The audit catalogued ~10 ad-hoc normalizers across LML/discogs-etl/semantic-index that took years to consolidate; we shouldn't add an eleventh.
- Option 1 keeps the canonical layer in `wxyc-etl`, builds on WX-2's primitives, and lets identity callers opt in explicitly via the function name.

### Public API

```rust
// In wxyc-etl/wxyc-etl/src/text/forms.rs (or a new identity.rs sibling).
pub fn to_identity_match_form(s: &str) -> String;

// Opt-in variants exposed if and only if the regression report (plan §3.3.4)
// puts the corresponding step over the per-step shift threshold:
pub fn to_identity_match_form_with_punctuation(s: &str) -> String;        // adds step 6
pub fn to_identity_match_form_with_disambiguator_strip(s: &str) -> String;  // adds step 8
```

PyO3 bindings mirror the Rust surface and ship under `wxyc_etl.text` alongside `to_match_form`.

### Algorithm (locked, derived from plan [§3.3.2](https://github.com/WXYC/wiki/blob/main/plans/library-hook-canonicalization-plan.md#332-algorithm-locked-except-where-noted))

```
to_identity_match_form(s) =
  1. m := to_match_form(s)              ← WX-2; covers steps 1-3 + 7 + sigma/folds/mojibake
  2. m := strip_trailing_parens(m)      ← step 4: ` (Remastered 2019)` and `\s*\[[^\]]*\]\s*$`
  3. m := drop_leading_article(m)       ← step 5: `^(the|a|an)\s+`
  4. m := trim(collapse_whitespace(m))  ← idempotent re-trim after the above edits
  RETURN m

to_identity_match_form_with_punctuation(s) = as above, with an additional
  step between (3) and (4): m := regex_replace(m, '[^\p{L}\p{N}\s]+', ' ')

to_identity_match_form_with_disambiguator_strip(s) = artist-only variant; same
  as base but with an additional final step: m := regex_replace(m, '\s*/\d+$', '')

to_identity_match_form_title(s) = base function; step-8 disambiguator strip
  is NEVER applied to titles regardless of opt-in (Discogs `/N` is artist-only).
```

The empty-output fallback specified by plan [§3.3.3](https://github.com/WXYC/wiki/blob/main/plans/library-hook-canonicalization-plan.md#333-parity-test-matrix-mandatory-before-any-cache-writes) (option B, locked) applies: if the result is empty, re-run with step 6 omitted; if still empty, return the original input verbatim with `norm_artist_fallback=true`. The hook table has a `CHECK norm_artist != ''` constraint backing this.

## Parity test matrix

Path: `wxyc-etl/tests/fixtures/identity_normalization_cases.csv` — input, expected_output, category, notes.
Test: `wxyc-etl/tests/identity_normalization_parity.rs` + SQL companion `identity_normalization_parity.sql`.
Vendoring: SHA-256-checked, identical to the WX-1 charset-torture corpus mechanism.

**Scope:** the cross-cache-identity-specific steps 4-6 + 8 ONLY. Steps 1-3 + 7's coverage is delegated to the WX-1 torture corpus at `wxyc-shared/src/test-utils/charset-torture.json` — duplicating those rows here would create maintenance drift.

**Categorical coverage** (per plan [§3.3.3](https://github.com/WXYC/wiki/blob/main/plans/library-hook-canonicalization-plan.md#333-parity-test-matrix-mandatory-before-any-cache-writes)):

| Category | Min rows | Tests |
|---|---|---|
| Trailing parens (artists) | 50 | step 4 |
| Trailing parens (titles) | 50 | step 4 with empty-paren and start-paren negatives |
| Leading articles | 25 | step 5 with `the`/`a`/`an`, including `Beatles, The` Discogs comma form |
| Punctuation-heavy artists | 50 | step 6 if shipped — `M.I.A.`, `!!!`, `+/-`, `O.D.B.`, `R.E.M.`, etc. |
| Trailing `/N` disambiguators (artists) | 50 | step 8 if shipped — `John Smith /1`, `Various /17` |
| Trailing `/N` (titles, must NOT strip) | 25 | step 8 negative — `Side A/B`, `Track 1/12` |
| Source-divergent rows | 250 | regression coverage; sampled where existing `norm_name` diverges from WX-2 + identity |

Failing parity blocks the wxyc-etl 0.3.x release tag.

## Regression report

Path: `wxyc-etl/tests/regression_report.rs`.
Output: `regression-report.json` written to `wxyc-etl/target/regression-report.json` and uploaded to the PR as a CI artifact.

**Baseline:** `to_match_form`. The shift attributable to going from the pre-WX-2 normalizer to `to_match_form` was accounted for in WX-2.3 per-repo migration; the identity report measures only the additional shift introduced by steps 4-6 + 8.

**Sections** per plan [§3.3.4](https://github.com/WXYC/wiki/blob/main/plans/library-hook-canonicalization-plan.md#334-regression-report-required-artifact) (lettered subsections retained for cross-reference):

- **A — Per-row diff:** rows where `to_identity_match_form(x) ≠ to_match_form(x)`, listing `(library_id, old_match_form, new_identity_form, which_steps_changed)`.
- **B — Per-step impact:** step 4, 5, 6, 8 in isolation. Rows changed only by step X (no other step would have changed them).
- **C — Identity match shift:** match LOST / GAINED / CHANGED counts when `flowsheet_match`, `fuzzy_resolved`, and `entity.identity` joins are recomputed under `to_identity_match_form`.
- **D — Per-step match shift:** the C analysis per-step.
- **E — Confidence threshold validation:** rows that would change confidence band (≥0.85 / 0.70-0.85 / <0.70) solely due to normalization shift.

**Acceptable thresholds** (plan [§3.3.4](https://github.com/WXYC/wiki/blob/main/plans/library-hook-canonicalization-plan.md#334-regression-report-required-artifact), reaffirmed):

- C-aggregate ≤2%: green; 2-5%: project-lead review; >5%: algorithm refinement before ship.
- D per step 4 / 5: any per-step shift triggers algorithm refinement, NOT opt-out (these are locked-on).
- D per step 6 / 8: shift >2% gates the step behind a separately-named opt-in function.

**Data source:** local Homebrew caches (Discogs full 62 GB, MusicBrainz 37 GB, Wikidata 262 MB). Read-only against prod-equivalent caches; never run against production RDS.

## Postgres analog: `wxyc_identity_match_artist`

WX-2 ships Rust + Python only. The Postgres-side function for use in expression indexes and cache builders is:

```sql
CREATE OR REPLACE FUNCTION wxyc_identity_match_artist(s text)
  RETURNS text LANGUAGE plpgsql IMMUTABLE PARALLEL SAFE
AS $$ ... $$;

CREATE OR REPLACE FUNCTION wxyc_identity_match_title(s text)
  RETURNS text LANGUAGE plpgsql IMMUTABLE PARALLEL SAFE
AS $$ ... $$;
```

The implementation mirrors `to_identity_match_form` byte-for-byte over the parity matrix. The vendored `wxyc_unaccent.rules` distribution mechanism, version-assertion DO-block, parity testing setup, per-cache + Backend implementation ownership table, and Postgres 16+ minimum version requirement are all defined verbatim in plan [§3.3.5](https://github.com/WXYC/wiki/blob/main/plans/library-hook-canonicalization-plan.md#335-postgres-analog-implementation-specification) and apply unchanged here.

The function name change (`wxyc_identity_match_artist` instead of plan [§3.3.5](https://github.com/WXYC/wiki/blob/main/plans/library-hook-canonicalization-plan.md#335-postgres-analog-implementation-specification)'s original `wxyc_norm_artist`) reflects the option-1 architectural decision: this function is for identity matching, not generic normalization.

Per-cache + Backend ownership matrix (cribbed from plan [§3.3.5](https://github.com/WXYC/wiki/blob/main/plans/library-hook-canonicalization-plan.md#335-postgres-analog-implementation-specification)):

| Environment | Owner repo | Migration file | Migration tool |
|---|---|---|---|
| Docker `discogs` (port 5433) | `WXYC/discogs-etl` | `discogs-etl/schema/00NN_wxyc_identity_match_functions.sql` | alembic |
| Homebrew `discogs` (port 5432, full) | `WXYC/discogs-etl` | (same file) | alembic |
| Homebrew `musicbrainz` | `WXYC/musicbrainz-cache` | `musicbrainz-cache/migrations/00NN_wxyc_identity_match_functions.sql` | sqlx-cli |
| Homebrew `wikidata` | `WXYC/wikidata-cache` | `wikidata-cache/migrations/00NN_wxyc_identity_match_functions.sql` | sqlx-cli |
| Backend (`wxyc_db`) | `WXYC/Backend-Service` | `shared/database/src/functions/identity_match.sql` (called from a Drizzle migration via `sql.raw()`) | Drizzle |

## Versioning

`to_identity_match_form` is a new public function. The plan's [§3.3.1](https://github.com/WXYC/wiki/blob/main/plans/library-hook-canonicalization-plan.md#331-sequenced-workflow-with-explicit-decision-gates) step 3 version-decision rule (bump if any of steps 4-6 + 8 ship) translates to: bump `wxyc-etl` from 0.2.x to **0.3.0** when this layer ships.

The 0.3.0 release published the new function alongside everything 0.2.x ships; no breaking change to existing API surface at that point. The legacy `normalize_artist_name` / `strip_diacritics` / `normalize_title` / `batch_normalize` were removed in 0.7.0 (per WX-4.1.1).

## Cross-references

- **Plan section:** [§3.3.0](https://github.com/WXYC/wiki/blob/main/plans/library-hook-canonicalization-plan.md#330-status-update-2026-05-06-wx-2-normalizer-charter-has-reshaped-this-section) status update; [§3.3.2](https://github.com/WXYC/wiki/blob/main/plans/library-hook-canonicalization-plan.md#332-algorithm-locked-except-where-noted) algorithm; [§3.3.3](https://github.com/WXYC/wiki/blob/main/plans/library-hook-canonicalization-plan.md#333-parity-test-matrix-mandatory-before-any-cache-writes) parity matrix; [§3.3.4](https://github.com/WXYC/wiki/blob/main/plans/library-hook-canonicalization-plan.md#334-regression-report-required-artifact) regression report; [§3.3.5](https://github.com/WXYC/wiki/blob/main/plans/library-hook-canonicalization-plan.md#335-postgres-analog-implementation-specification) Postgres analog.
- **Audit deliverable:** [`docs/normalization-audit.md`](./normalization-audit.md) (closed `wxyc-etl#74` via PR [#88](https://github.com/WXYC/wxyc-etl/pull/88)).
- **WX-2 contracts:** [WXYC/docs#16](https://github.com/WXYC/docs/issues/16).
- **Cross-cache-identity epic:** [WXYC/Backend-Service#663](https://github.com/WXYC/Backend-Service/issues/663) (E2-BS) and [WXYC/wxyc-etl#73](https://github.com/WXYC/wxyc-etl/issues/73) (E3).
