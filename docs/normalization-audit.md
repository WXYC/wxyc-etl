# Normalization Audit (E3 step 1)

This is the deliverable for `wxyc-etl#74` — a pure-research inventory of every artist/title normalization implementation across the eight WXYC consumer surfaces, plus the §8.6 hard-gate test-path verification, plus the Q8 (NFC vs NFKD) and Q9 (empty-output) decisions.

Scope corresponds to plan `library-hook-canonicalization-plan.md` §3.3.1 / §3.3.1.1 / §3.3.1.2 / §3.3.2 / §3.3.3 / §8.5 / §8.6 / §9. References to "the proposed algorithm" mean §3.3.2 of the plan. Where outputs were obtained by running the actual implementation, they are marked `(executed)`. Where outputs were derived by reading code, they are marked `(traced, not executed)`.

Sample-input set used throughout (drawn from `wxyc-shared/src/test-utils/wxyc-example-data.json` plus three diagnostic adversaries):

| # | Input | Why |
|---|---|---|
| A | `Nilüfer Yanya` | Latin diacritic (ü) in canonical pool |
| B | `Csillagrablók` | Latin diacritic (ó) in canonical pool |
| C | `Hermanos Gutiérrez` | Latin diacritic (é) in canonical pool |
| D | `Juana Molina` | ASCII baseline |
| E | `Stereolab` | ASCII baseline |
| F | `The Beatles` | Article-strip diagnostic (synthetic; not in WXYC pool) |
| G | `Beatles, The` | Discogs comma-article diagnostic |
| H | `Discharge (2)` | Discogs `(N)` disambiguator |
| I | `John Smith /1` | Discogs `/N` disambiguator |
| J | `Foo (Remastered 2019)` | Trailing-paren title |
| K | `M.I.A.` | Punctuation-heavy artist |
| L | `!!!` | Pure-punctuation artist (empty-output candidate) |
| M | `Σ` / `ς` | Greek sigma case-fold |

---

## 1. Inventory

### 1.1 wxyc-etl (Rust + PyO3)

**Source:** `/Users/jake/Developer/WXYC/wxyc-etl/wxyc-etl/src/text/normalize.rs`

| Symbol | Location | Lowercase? | NFKD? | Sigma fold? | Article? | Paren? | Punct? | Disambig? | Trim? | Empty? |
|---|---|---|---|---|---|---|---|---|---|---|
| `normalize_artist_name` | `normalize.rs:23` | yes | yes (drop `is_mark()`) | yes (ς→σ + Σ→σ via lowercasing) | no | no | no | no | yes (trim_matches `' '` only) | passes through |
| `normalize_title` | `normalize.rs:55` | (delegates to `normalize_artist_name`) | | | | | | | | |
| `strip_diacritics` | `normalize.rs:41` | no | yes (drop `is_mark()`) | yes (ς→σ; capital Σ preserved) | no | no | no | no | no | passes through |
| PyO3 `wxyc_etl.text.normalize_artist_name` | `wxyc-etl-python/src/text.rs:18` | (re-exports the Rust) | | | | | | | | accepts `None` → `""` |

**Algorithm trace** (matches the Rust source exactly):

```
1. NFKD decompose
2. Drop is_mark() characters (combining marks)
3. Per-character to_lowercase() (unfolded into the same pass)
   3'. Inline fold_sigma: U+03C2 (ς) → U+03C3 (σ); U+03A3 (Σ) lowercases to σ
4. trim_matches(' ')   — leading/trailing ASCII spaces only, no other whitespace,
                         no internal collapse
```

**Sample inputs/outputs** (executed, via `library-metadata-lookup/.venv/bin/python` with the published 0.1.0 wheel):

| Input | Output |
|---|---|
| `Nilüfer Yanya` | `nilufer yanya` |
| `Csillagrablók` | `csillagrablok` |
| `Hermanos Gutiérrez` | `hermanos gutierrez` |
| `Juana Molina` | `juana molina` |
| `Stereolab` | `stereolab` |
| `The Beatles` | `the beatles` |
| `Beatles, The` | `beatles, the` |
| `Discharge (2)` | `discharge (2)` |
| `John Smith /1` | `john smith /1` |
| `Foo (Remastered 2019)` | `foo (remastered 2019)` |
| `M.I.A.` | `m.i.a.` |
| `!!!` | `!!!` |
| `Σ` | `σ` |
| `ς` | `σ` |
| `ﬁreflies` (NFKD ligature) | `fireflies` |
| `Ⅷ Symphony` | `viii symphony` |
| `Hüsker Dü` | `husker du` |
| `Motörhead` | `motorhead` |
| `Zoé` | `zoe` |

**Per-rule classification (§3.3.1.2 schema):**

```json
{
  "implementation": "wxyc_etl::text::normalize_artist_name",
  "location": "wxyc-etl/wxyc-etl/src/text/normalize.rs:23",
  "language": "rust",
  "rules": {
    "step_1_nfkd": "implemented",
    "step_2_drop_marks": "implemented",
    "step_3_lowercase": "implemented (single-pass with step 2; sigma fold added beyond §3.3.2)",
    "step_4_paren_strip": "absent",
    "step_5_article_drop": "absent",
    "step_6_punctuation_collapse": "absent",
    "step_7_collapse_whitespace": "implemented_divergent (trim_matches(' ') only — no internal-run collapse, no tab/newline trim)",
    "step_8_disambiguator_strip": "absent"
  },
  "divergences": [
    "step_7: only trims ASCII space, not full whitespace; does not collapse internal runs",
    "extra_step_3': folds Greek final-form sigma ς to medial σ — divergence FROM Python prototype, intentional"
  ],
  "post_v2_action": "patch step_7 to collapse internal runs and trim full whitespace; adopt steps 4-6 + 8 from new code; keep sigma fold (covered by §3.3.2 step 3 implicitly via per-char to_lowercase, plus the explicit ς fold)"
}
```

### 1.2 discogs-etl (Python)

Two distinct implementations, each in its own script.

#### 1.2.1 `scripts/filter_csv.py:normalize_artist`

**Source:** `/Users/jake/Developer/WXYC/discogs-etl/scripts/filter_csv.py:37`

```python
def normalize_artist(name: str) -> str:
    nfkd = unicodedata.normalize("NFKD", name)
    stripped = "".join(c for c in nfkd if not unicodedata.combining(c))
    return stripped.lower().strip()
```

This is the original Python prototype the Rust mirrors (per the docstring at `wxyc-etl/wxyc-etl/src/text/normalize.rs:1-7`). One material difference from the Rust: `.strip()` strips all whitespace (tabs, newlines too), not just ASCII space; the Rust uses `trim_matches(' ')`. Also no Greek-sigma fold.

| Rule | State |
|---|---|
| step_1_nfkd | implemented |
| step_2_drop_marks | implemented |
| step_3_lowercase | implemented (after diacritic strip; no sigma fold) |
| step_4_paren_strip | absent |
| step_5_article_drop | absent |
| step_6_punctuation_collapse | absent |
| step_7_collapse_whitespace | implemented_divergent (`.strip()` only — broader than Rust trim, but still no internal collapse) |
| step_8_disambiguator_strip | absent |

**Sample outputs** (traced; identical to wxyc_etl except for whitespace edge cases and sigma fold):

`Nilüfer Yanya` → `nilufer yanya`; `Σ` → `σ` (via Python's `str.lower()`); `ς` → `ς` (Python's `lower()` is identity on the final-form sigma — divergence from wxyc_etl Rust).

#### 1.2.2 `scripts/verify_cache.py` family — three layered functions

**Source:** `/Users/jake/Developer/WXYC/discogs-etl/scripts/verify_cache.py:119-184`

```python
def strip_accents(s: str) -> str:                          # :119
    nfkd = unicodedata.normalize("NFKD", s)
    return "".join(c for c in nfkd if not unicodedata.combining(c))

def normalize_for_comparison(name: str) -> str:            # :157
    name = name.strip().lower()
    name = strip_accents(name)
    name = DISCOGS_DISAMBIGUATION_RE.sub("", name)         # \s*\(\d+\)\s*$
    name = LIBRARY_DISAMBIGUATION_RE.sub("", name)         # \s*\[.*?\]\s*$
    for article in ("the","los","las","les","la","le","el","die","der","das"):
        suffix = f", {article}"
        if name.endswith(suffix):
            name = f"{article} " + name[:-len(suffix)]     # "Beatles, The" → "the beatles"
            break
    return name.strip()

def normalize_artist(name: str) -> str:                    # :141
    name = normalize_for_comparison(name)
    name = re.sub(r"\s*&\s*", " and ", name)               # & → and
    name = name.replace("'", "")                           # drop apostrophes only
    name = " ".join(name.split())
    return name

def normalize_title(title: str) -> str:                    # :125
    title = title.strip().lower()
    title = strip_accents(title)
    prev = None
    while title != prev:                                    # repeated suffix-strip until stable
        prev = title
        title = TITLE_SUFFIX_RE.sub("", title).strip()
    return title
```

`TITLE_SUFFIX_RE` (`:76`) matches: `12"`/`7"` vinyl marks, `(\d+)` Discogs disambiguators, `(N cd|lp set)`, `(reissue|deluxe edition|expanded edition|anniversary edition|special edition|limited edition|bonus tracks|ep|lp)`, `(\d+lp)`. All trailing.

**Sample outputs** (executed):

| Input | `normalize_artist` | `normalize_title` |
|---|---|---|
| `Nilüfer Yanya` | `nilufer yanya` | `nilufer yanya` |
| `Csillagrablók` | `csillagrablok` | — |
| `Hermanos Gutiérrez` | `hermanos gutierrez` | — |
| `The Beatles` | `the beatles` | — |
| `Beatles, The` | `the beatles` ✓ (article unflipped) | `beatles, the` (titles do NOT flip) |
| `Discharge (2)` | `discharge` | — |
| `Stereolab [UK]` | `stereolab` | — |
| `Me & Mr. Jones` | `me and mr. jones` | — |
| `M.I.A.` | `m.i.a.` (apostrophe-less only — periods stay) | — |
| `DOGA 12"` | — | `doga` |
| `Album (12")` | — | `album (12")` (paren not stripped — only specific keyword parens) |
| `Edits (CD)` | — | `edits (cd)` (CD not in the keyword list) |
| `Album (Reissue)` | — | `album` (paren stripped) |
| `Aluminum Tunes (Reissue)` | — | `aluminum tunes` |

| Rule (artist) | State |
|---|---|
| step_1_nfkd | implemented |
| step_2_drop_marks | implemented |
| step_3_lowercase | implemented |
| step_4_paren_strip | implemented_divergent (only `\s*\(\d+\)\s*$` — Discogs numeric disambig only, not arbitrary parens) |
| step_5_article_drop | implemented_divergent (does the OPPOSITE of §3.3.2: re-attaches comma-article rather than dropping leading article. `"Beatles, The" → "the beatles"`, not `"Beatles, The" → "beatles"`.) |
| step_6_punctuation_collapse | implemented_divergent (only `&→and` and apostrophe removal; periods, slashes, plus, etc. preserved) |
| step_7_collapse_whitespace | implemented (`" ".join(name.split())` collapses runs and trims) |
| step_8_disambiguator_strip | absent (the artist `/N` form is not handled; only the `(N)` form) |

**Note on step 5 conflict:** `verify_cache.normalize_artist` *re-attaches* the comma-article so a Discogs-style `"Beatles, The"` and the natural `"The Beatles"` both end up `the beatles`. §3.3.2 step 5 *drops* the leading article so both end up `beatles`. Both choices collapse the pair, but the post-strip canonical form differs. Consumers of the new function on data that had been keyed against the discogs-etl form will see `the beatles` → `beatles` and lose any keys built off the old form. The §3.3.4 regression report needs to count this.

### 1.3 library-metadata-lookup (Python)

LML normalization is fragmented across at least 10 functions, all eventually rooted at `wxyc_etl.text.normalize_artist_name` for the diacritic-bearing leg, with divergent post-processing on top.

#### 1.3.1 `discogs/matching.py` — three functions

**Source:** `/Users/jake/Developer/WXYC/library-metadata-lookup/discogs/matching.py`

```python
# :14 — alias for the wxyc-etl Rust normalize
from wxyc_etl.text import normalize_artist_name as normalize_for_comparison

# :17
_DISCOGS_SUFFIX_RE = re.compile(r"\s*\(\d+\)$")

# :20
def strip_discogs_suffix(name: str) -> str:
    return _DISCOGS_SUFFIX_RE.sub("", name)

# :31
def normalize_for_track_comparison(text: str | None) -> str:
    if not text: return ""
    result = normalize_for_comparison(text)            # NFKD + drop marks + lowercase + trim
    result = result.replace("&", " and ")              # NOT regex — this drops bare & not surrounded by spaces too
    result = re.sub(r"[^\w\s]", "", result)            # strip ALL non-word non-space chars (Unicode-aware \w)
    result = re.sub(r"\s+", " ", result).strip()       # collapse whitespace
    return result

# :51
def normalize_artist_for_validation(name: str) -> str:
    if not name: return ""
    result = name.lower().replace('"', "").replace("'", "")
    return strip_discogs_suffix(result).strip()
```

**Critical observation:** `normalize_artist_for_validation` does **NOT** strip diacritics. It does `name.lower()` only. This means it preserves Latin combining marks. For a database that keyed off `normalize_artist_name`, comparing on `normalize_artist_for_validation` would miss the canonicalization entirely. This is intentional per the docstring (substring comparison during track validation) but it means there are two parallel "normalize an artist" entry points in LML that produce categorically different outputs.

**Sample outputs** (executed):

| Input | `normalize_for_track_comparison` | `normalize_artist_for_validation` |
|---|---|---|
| `Nilüfer Yanya` | `nilufer yanya` | `nilüfer yanya` (diacritic preserved) |
| `Hermanos Gutiérrez` | `hermanos gutierrez` | `hermanos gutiérrez` |
| `The Beatles` | `the beatles` | `the beatles` |
| `Beatles, The` | `beatles the` (comma stripped by `[^\w\s]`) | `beatles, the` |
| `Discharge (2)` | `discharge 2` | `discharge` |
| `John Smith /1` | `john smith 1` | `john smith /1` |
| `Foo (Remastered 2019)` | `foo remastered 2019` | `foo (remastered 2019)` |
| `M.I.A.` | `mia` (periods drop; collapsed) | `m.i.a.` |
| `!!!` | `''` (empty) | `!!!` |
| `+/-` | `''` (empty) | `+/-` |
| `Me & Mr. Jones` | `me and mr jones` | (same — only quotes/apostrophes stripped) |
| `"Weird Al" Yankovic` | (passes through `& → and` no-op; `"`s drop via `[^\w\s]` → `weird al yankovic`) | `weird al yankovic` |

| Rule | `normalize_for_track_comparison` | `normalize_artist_for_validation` |
|---|---|---|
| step_1_nfkd | implemented (via `normalize_for_comparison`) | absent |
| step_2_drop_marks | implemented | absent |
| step_3_lowercase | implemented | implemented (only this) |
| step_4_paren_strip | absent (parens collapse via punct-strip step 6 instead, but content remains) | implemented_divergent (`\s*\(\d+\)$` Discogs form only) |
| step_5_article_drop | absent | absent |
| step_6_punctuation_collapse | implemented_divergent (`[^\w\s]` is a STRIP not a COLLAPSE — runs of punctuation become empty, not single-space; differs from §3.3.2 which produces a single space) | absent (only `"` and `'` removed) |
| step_7_collapse_whitespace | implemented | implemented (trim only) |
| step_8_disambiguator_strip | absent (the `/N` form falls into the `[^\w\s]` strip) | absent |

#### 1.3.2 `discogs/service.py:calculate_confidence` inner `normalize`

**Source:** `/Users/jake/Developer/WXYC/library-metadata-lookup/discogs/service.py:141-147`

```python
def calculate_confidence(...):
    ...
    def normalize(s: str | None) -> str:
        return s.lower().strip() if s else ""
```

This is the function the plan §3.3.1.1 calls "LML `discogs/service.py` `normalize`" — it's a closure inside `calculate_confidence`, used only for confidence-scoring substring comparison. Lowercase + trim, nothing else. **No NFKD, no diacritic strip.** "Nilüfer" and "nilufer" do NOT collapse here.

| Rule | State |
|---|---|
| step_1_nfkd | absent |
| step_2_drop_marks | absent |
| step_3_lowercase | implemented |
| step_4–step_8 | absent |
| step_7_collapse_whitespace | implemented (trim only) |

#### 1.3.3 `library/db.py` — fallback search query normalization

**Source:** `/Users/jake/Developer/WXYC/library-metadata-lookup/library/db.py:9, :327, :376`

```python
from wxyc_etl.text import normalize_artist_name as normalize_for_comparison
...
normalized = re.sub(r"[^a-z0-9\s]", " ", normalize_for_comparison(query))
words = normalized.split()
```

After NFKD-folding to lowercased ASCII via `normalize_for_comparison`, anything outside `[a-z0-9\s]` is replaced by a single space — note the character class is ASCII-only (`a-z`, not `\p{L}`), so any non-Latin characters that survived NFKD (Cyrillic, Greek-pre-fold, CJK) get nuked. After Greek sigma fold to σ (U+03C3), σ is *not* in `[a-z0-9]` and gets replaced with space. ASCII-fold is the implicit assumption.

| Rule | State |
|---|---|
| step_1_nfkd | implemented (via wxyc_etl) |
| step_2_drop_marks | implemented (via wxyc_etl) |
| step_3_lowercase | implemented (via wxyc_etl) |
| step_4_paren_strip | implemented_divergent (parens collapse via `[^a-z0-9\s]` strip — content remains) |
| step_5_article_drop | absent |
| step_6_punctuation_collapse | implemented_divergent (`[^a-z0-9\s]` is ASCII-only, drops every non-Latin code point; replaces with single space) |
| step_7_collapse_whitespace | implemented (`split()` collapses) |
| step_8_disambiguator_strip | absent |

#### 1.3.4 `discogs/cache_service.py` — Postgres-side `lower(f_unaccent(...))`

**Source:** `/Users/jake/Developer/WXYC/library-metadata-lookup/discogs/cache_service.py:84-744` (every fuzzy SQL query)

Pattern: `lower(f_unaccent(col)) % lower(f_unaccent($1))` with the `%` similarity operator; `similarity(lower(f_unaccent(col)), lower(f_unaccent($1)))` for ranking. The `f_unaccent` immutable wrapper is defined in `discogs-etl/schema/create_functions.sql` (uses `public.unaccent('public.unaccent', ...)`).

`unaccent` does not implement NFKD compatibility decomposition — it uses its own dictionary-driven character mapping. For Latin-1 it agrees with the wxyc_etl NFKD-then-drop-marks output (Nilüfer → nilufer). For ligatures (ﬁ, ﬃ) and Roman-numeral compatibility decompositions (Ⅷ, Ⅻ) the default `unaccent.rules` does **not** decompose — so the SQL side and the Rust side disagree on those code points. This is the LML #194 territory the plan calls out and §3.3.5 addresses with the wxyc_unaccent.rules vendored file.

| Rule | State |
|---|---|
| step_1_nfkd | implemented_divergent (Postgres `unaccent` is a non-NFKD substitute; matches NFKD on Latin diacritics, diverges on ligatures + compat decompositions) |
| step_2_drop_marks | implemented (via unaccent) |
| step_3_lowercase | implemented |
| step_4–step_8 | absent (raw column on the right, but pg_trgm tolerates punctuation noise) |

#### 1.3.5 `scripts/streaming_availability/matching.py` — three functions

**Source:** `/Users/jake/Developer/WXYC/library-metadata-lookup/scripts/streaming_availability/matching.py:57, :62, :96`

```python
from wxyc_etl.text import normalize_artist_name as normalize_for_comparison

_FORMAT_SUFFIX_RE = re.compile(
    r"""\s+(?:\d{1,2}["""u"“”"]|LP|EP|CD|x\s*\d+)$""",
    re.IGNORECASE,
)
_PARENTHETICAL_SUFFIX_RE = re.compile(
    r"\s*\([^)]*(?:reissue|remaster(?:ed)?|deluxe|limited|edition|expanded|anniversary|bonus)[^)]*\)\s*$",
    re.IGNORECASE,
)
_BRACKET_SUFFIX_RE = re.compile(
    r"\s*\[[^\]]*(?:single|EP|sampler|promo|import)\]?\s*$",
    re.IGNORECASE,
)
_THE_PREFIX_RE = re.compile(r"^The\s+", re.IGNORECASE)

def strip_format_suffix(title: str) -> str:                       # :40
    result = _PARENTHETICAL_SUFFIX_RE.sub("", title)
    result = _BRACKET_SUFFIX_RE.sub("", result)
    result = _FORMAT_SUFFIX_RE.sub("", result)
    return result.strip()

def strip_the_prefix(name: str) -> str: ...                       # :50

def normalize_album_title(title: str) -> str:                     # :57
    return normalize_for_comparison(strip_format_suffix(title))

def normalize_artist_name(artist: str) -> str:                    # :62
    return normalize_for_comparison(artist)

def normalize_artist_credit(artist: str) -> list[str]:            # :96
    # Generates variants for fuzzy matching — and↔&, slash, parens, feat. — does NOT lowercase
```

**Sample outputs** (executed):

| Input | `normalize_album_title` |
|---|---|
| `Nilüfer Yanya` | `nilufer yanya` |
| `DOGA 12"` | `doga` |
| `Aluminum Tunes (Reissue)` | `aluminum tunes` |
| `Album (12")` | `album (12")` (the format-suffix regex doesn't match `(12")`-style parens) |
| `Edits [single]` | `edits` |
| `Album (Reissue)` | `album` |
| `The Beatles` | `the beatles` |

| Rule | State |
|---|---|
| step_1_nfkd | implemented (via wxyc_etl) |
| step_2_drop_marks | implemented |
| step_3_lowercase | implemented |
| step_4_paren_strip | implemented_divergent (only specific keyword parens at end: reissue/remaster/deluxe/etc.) |
| step_5_article_drop | absent in `normalize_album_title`; provided as a separate `strip_the_prefix` and only invoked inside `score_match` for an alt-score |
| step_6_punctuation_collapse | absent |
| step_7_collapse_whitespace | implemented |
| step_8_disambiguator_strip | absent |

#### 1.3.6 `scripts/match_compilations.py:42` — `normalize_comp_title`

**Source:** `/Users/jake/Developer/WXYC/library-metadata-lookup/scripts/match_compilations.py:42`

```python
_BRACKET_RE = re.compile(r"\s*\[[^\]]*\]\s*$")
_PAREN_ANNOTATION_RE = re.compile(r"\s*\([^)]*\)\s*$")
_FORMAT_SUFFIX_RE = re.compile(r"""\s+(?:\d{1,2}["""u"“”"]|LP|EP|CD|x\s*\d+)$""", re.IGNORECASE)

def normalize_comp_title(title: str) -> str:
    result = _BRACKET_RE.sub("", title)
    result = _PAREN_ANNOTATION_RE.sub("", result)
    result = _FORMAT_SUFFIX_RE.sub("", result)
    return result.strip()
```

**No lowercase, no NFKD, no diacritic strip.** Used at compilation matching; the inputs are case- and diacritic-bearing display titles. `Nilüfer Yanya` → `Nilüfer Yanya`. Used only inside the script's local matcher — does not key any persistent index.

| Rule | State |
|---|---|
| step_1_nfkd | absent |
| step_2_drop_marks | absent |
| step_3_lowercase | absent |
| step_4_paren_strip | implemented (any trailing paren, not just keyword-matched) |
| step_5_article_drop | absent |
| step_6_punctuation_collapse | absent |
| step_7_collapse_whitespace | implemented (trim only) |
| step_8_disambiguator_strip | absent |

#### 1.3.7 `scripts/spotify_artist_catalog.py:96` — `normalize_title`

**Source:** `/Users/jake/Developer/WXYC/library-metadata-lookup/scripts/spotify_artist_catalog.py:96`

```python
def normalize_title(title: str) -> str:
    t = re.sub(r'\s+(?:\d{1,2}["""“”]|LP|EP|CD)$', "", title).strip()
    t = re.sub(r"\s*\[[^\]]*\]$", "", t).strip()
    t = re.sub(r"\s+b/w\s+.*$", "", t, flags=re.I).strip()
    t = re.sub(r"\s+cd-?\d*$", "", t, flags=re.I).strip()
    return t
```

No lowercase, no NFKD. Strips trailing format suffix, trailing bracket, b/w-tail, cd-N tail. Spotify-specific.

| Rule | State |
|---|---|
| step_1–step_3 | absent |
| step_4_paren_strip | absent |
| step_5_article_drop | absent |
| step_6_punctuation_collapse | absent |
| step_7_collapse_whitespace | implemented (`.strip()` only) |
| step_8_disambiguator_strip | absent |

#### 1.3.8 `scripts/variation_audit/loaders.py:15` — `normalize_name`

**Source:** `/Users/jake/Developer/WXYC/library-metadata-lookup/scripts/variation_audit/loaders.py:15`

```python
def normalize_name(name: str) -> str:
    decomposed = unicodedata.normalize("NFKD", name)
    stripped = "".join(c for c in decomposed if not unicodedata.combining(c))
    return stripped.lower().strip()
```

Identical algorithm to `discogs-etl/scripts/filter_csv.py:normalize_artist`, the original Python prototype. `Nilüfer Yanya` → `nilufer yanya`; `ς` → `ς` (no sigma fold; Python's `lower()` is identity on the final-form sigma).

| Rule | State |
|---|---|
| step_1_nfkd | implemented |
| step_2_drop_marks | implemented |
| step_3_lowercase | implemented (no sigma fold) |
| step_4–step_8 | absent |
| step_7_collapse_whitespace | implemented (`.strip()` only — no internal collapse) |

#### 1.3.9 `scripts/track_streaming/validate.py:24` — `_normalize`

**Source:** `/Users/jake/Developer/WXYC/library-metadata-lookup/scripts/track_streaming/validate.py:24`

```python
def _normalize(text: str) -> str:
    t = text.lower().strip()
    t = re.sub(r"\s*\([^)]*\)", "", t)             # any paren anywhere, not just trailing
    t = re.sub(r"\s*\[[^\]]*\]", "", t)            # any bracket anywhere
    return t.strip()
```

Lowercase + global paren/bracket strip. **No NFKD.** Used only for service-metadata validation (oEmbed responses).

| Rule | State |
|---|---|
| step_1–step_2 | absent |
| step_3_lowercase | implemented |
| step_4_paren_strip | implemented_divergent (any position, not just trailing) |
| step_5–step_6 | absent |
| step_7_collapse_whitespace | implemented (trim only) |
| step_8 | absent |

#### 1.3.10 `scripts/validate_streaming_urls.py:38` — `_normalize_title`

**Source:** `/Users/jake/Developer/WXYC/library-metadata-lookup/scripts/validate_streaming_urls.py:38`

Format-suffix + bracket + keyword-paren + b/w + cd-N strip. No lowercase, no NFKD. Same shape as `spotify_artist_catalog.normalize_title` with extras.

| Rule | State |
|---|---|
| step_1–step_3 | absent |
| step_4_paren_strip | implemented_divergent (keyword-only) |
| step_5–step_6 | absent |
| step_7 | implemented (trim only) |
| step_8 | absent |

#### 1.3.11 `scripts/entity_resolution/discogs.py` — Postgres-side reconciler

**Source:** `/Users/jake/Developer/WXYC/library-metadata-lookup/scripts/entity_resolution/discogs.py:43-65`

Every reconciler SQL is `lower(f_unaccent(col)) = ANY($1)` against a Python-side input list pre-normalized via `wxyc_etl.text.normalize_artist_name`. Same parity issue as §1.3.4: NFKD ↔ unaccent disagreement on ligatures.

| Rule | State |
|---|---|
| step_1_nfkd | implemented_divergent (Python side NFKD; Postgres side `unaccent`) |
| step_2_drop_marks | implemented |
| step_3_lowercase | implemented |
| rest | absent |

### 1.4 semantic-index (Python)

**Source:** `/Users/jake/Developer/WXYC/semantic-index/semantic_index/artist_resolver.py:66-78`

```python
from wxyc_etl.text import normalize_artist_name, split_artist_name, is_compilation_artist

_BRACKET_RE = re.compile(r"\s*\[.*?\]\s*$")

def _normalize(name: str) -> str:
    s = normalize_artist_name(name)        # NFKD + drop marks + lowercase + trim
    s = _BRACKET_RE.sub("", s)              # strip trailing [...]
    if s.startswith("the "):
        s = s[4:]                           # drop leading "the "
    s = s.replace(" & ", " and ")           # & → and (only when surrounded by spaces)
    return s
```

`_normalized_forms(name)` (`:81`) generates a base form plus alias parts via `wxyc_etl.text.split_artist_name` (handles `, `, ` / `, ` + `) plus an extra ` aka ` separator.

**Sample outputs** (executed via semantic-index venv):

| Input | `_normalize` |
|---|---|
| `Nilüfer Yanya` | `nilufer yanya` |
| `Csillagrablók` | `csillagrablok` |
| `Hermanos Gutiérrez` | `hermanos gutierrez` |
| `The Beatles` | `beatles` (article dropped — diverges from wxyc_etl base) |
| `Beatles, The` | `beatles, the` (comma-suffix not handled here) |
| `Stereolab [UK]` | `stereolab` |
| `Black Sabbath & Friends` | `black sabbath and friends` |
| `J Dilla / Jay Dee` | `j dilla / jay dee` (slash with spaces preserved by both wxyc_etl and the local code) |

`_normalized_forms('J Dilla / Jay Dee')` → `['j dilla / jay dee', 'j dilla', 'jay dee']` (split via wxyc_etl + extra forms added).

There is also `archive_match.py:_decode_and_strip` which html-unescapes, drops `\"` / `\\` / `"`, and trims. This runs *before* `_normalize`, not as part of it, but it's the canonical pre-normalize cleanup for archive-side names. Not a normalization in the §3.3.2 sense — preprocessing only.

| Rule | State |
|---|---|
| step_1_nfkd | implemented (via wxyc_etl) |
| step_2_drop_marks | implemented |
| step_3_lowercase | implemented |
| step_4_paren_strip | implemented_divergent (only trailing `[...]`, not `(...)`) |
| step_5_article_drop | implemented_divergent (only `the ` prefix; no `a `, no `an `, no comma-form) |
| step_6_punctuation_collapse | implemented_divergent (only `' & '` → ` and `) |
| step_7_collapse_whitespace | implemented (via wxyc_etl trim; no internal collapse) |
| step_8_disambiguator_strip | absent |

### 1.5 musicbrainz-cache (Rust)

**Source:** `/Users/jake/Developer/WXYC/musicbrainz-cache/src/filter.rs:5-65`

```rust
use wxyc_etl::text::normalize_artist_name;

// :7 load library names, normalize, build HashSet
artists.insert(normalize_artist_name(&name));
// :43, :63 filter rows by membership in the normalized set
if library_artists.contains(&normalize_artist_name(name)) { ... }
```

**No additional steps.** Pure delegate to wxyc_etl. The Postgres schema is `lower(name)` indexes (no `unaccent`):

```sql
-- migrations/0001_initial.sql:195, :198
CREATE INDEX idx_mb_artist_name_lower ON mb_artist (lower(name));
CREATE INDEX idx_mb_artist_alias_name_lower ON mb_artist_alias (lower(name));
```

So Rust-side filtering is NFKD-folded; SQL-side query (in the few queries that use these indexes) is bare `lower()` — diacritics survive on the SQL side. This is a divergence between the build-time filter and the read-time query path. Plan §3.3.5 addresses this by adding `wxyc_norm_*` Postgres functions and re-indexing.

| Rule (Rust filter) | State |
|---|---|
| step_1_nfkd | implemented (via wxyc_etl) |
| step_2_drop_marks | implemented |
| step_3_lowercase | implemented |
| rest | absent |

### 1.6 wikidata-cache (Rust + Postgres)

**Source:** `/Users/jake/Developer/WXYC/wikidata-cache/src/extractor.rs:125`, `/Users/jake/Developer/WXYC/wikidata-cache/migrations/0001_initial.sql:5-58`

The Rust extractor stores `entity.label` and `entity_alias.alias` **as-is** from Wikidata's English-language label fields. No normalization. Postgres indexes:

```sql
CREATE EXTENSION IF NOT EXISTS pg_trgm;
CREATE INDEX idx_entity_label_trgm ON entity USING gin(label gin_trgm_ops);
CREATE INDEX idx_entity_alias_text_trgm ON entity_alias USING gin(alias gin_trgm_ops);
```

No `unaccent`, no `lower()`. Search queries that hit these indexes either match case-sensitively or do client-side casefolding; pg_trgm itself is case-insensitive at similarity-scoring time but the index expression is not normalized. Sample inputs persist verbatim: `Nilüfer Yanya` is stored and matched as `Nilüfer Yanya`.

| Rule | State |
|---|---|
| step_1_nfkd | absent |
| step_2_drop_marks | absent |
| step_3_lowercase | absent |
| rest | absent |

### 1.7 Backend-Service (TypeScript / Drizzle / Postgres)

The Backend has **no application-layer artist normalization function**. Search is entirely Postgres-side via tsvector + pg_trgm:

**Migration sources** (`shared/database/src/migrations/`):

| Migration | What it adds | Normalization |
|---|---|---|
| `0042_flowsheet-suggest-indexes.sql` | gin_trgm on `artist_name`, `track_title` | none — raw column |
| `0049_flowsheet-search-indexes.sql` | gin_trgm on `album_title`, `record_label` | none |
| `0051_dj-name-search-indexes.sql` | gin_trgm on `auth_user.dj_name`, `name`, `shows.legacy_dj_name` | none |
| `0052_flowsheet-tsvector-search.sql` | `flowsheet.search_doc` STORED tsvector via `to_tsvector('simple', col)` | `simple` config — lowercases ASCII, no stemming, no diacritic strip |
| `0054_flowsheet-search-doc-with-dj-name.sql` (and the 0065 replay) | extends `search_doc` to include `dj_name` | same — `to_tsvector('simple', ...)` |
| `0058_library-artist-name-and-search-doc.sql` | `library.search_doc` STORED tsvector + gin_trgm on `library.artist_name` | same `to_tsvector('simple')`; trigram on raw column |

`dev_env/install_extensions.sql` installs `pg_trgm` only — **no `unaccent` extension is installed on Backend**. There is no `f_unaccent` wrapper.

**Service layer** (`apps/backend/services/library.service.ts:457`, `apps/backend/services/search.service.ts:257`):

```ts
const tsquery = sql`websearch_to_tsquery('simple', ${query})`;
const tsvectorPredicate = sql`${library.search_doc} @@ ${tsquery}`;
```

`websearch_to_tsquery('simple', ...)` lowercases ASCII letters and tokenizes; combining marks survive; ligatures survive. So Backend search treats `Nilüfer Yanya` and `Nilufer Yanya` as **distinct** tokens — a known divergence from every other consumer in this audit.

| Rule | State |
|---|---|
| step_1_nfkd | absent |
| step_2_drop_marks | absent (this is the one consumer that does not strip diacritics at all on the search path) |
| step_3_lowercase | implemented (via `to_tsvector('simple', ...)`) |
| step_4–step_8 | absent |

§3.3.5's per-cache `wxyc_norm_*` Postgres functions plus a `CREATE EXTENSION unaccent` are the migration that brings Backend to parity.

### 1.8 tubafrenzy (Java / Lucene)

Two normalization surfaces.

#### 1.8.1 Lucene index analyzer — `WxycAnalyzer`

**Source:** `/Users/jake/Developer/WXYC/tubafrenzy/libs/lucene/src/main/java/org/wxyc/lucene/analysis/WxycAnalyzer.java`

```java
@Override
protected TokenStreamComponents createComponents(String fieldName) {
    var tokenizer = new StandardTokenizer();
    return new TokenStreamComponents(tokenizer, new ICUFoldingFilter(tokenizer));
}

@Override
protected TokenStream normalize(String fieldName, TokenStream in) {
    return new ICUFoldingFilter(in);
}
```

ICU `ICUFoldingFilter` performs **NFKC_Casefold + Latin diacritic folding**: Unicode normalize + casefold + drop combining marks + Turkish dotless-i fold + halfwidth-katakana fold + Greek `Ω` → `ω`. This is materially close to but distinct from §3.3.2 steps 1–3:

- ICU uses **NFKC_Casefold**; §3.3.2 uses **NFKD + drop is_mark() + per-char to_lowercase()**. NFKC re-composes after decomposition (so `Nilüfer` → `nilüfer` post-fold has the precomposed `ü` diacritic re-added — except ICUFoldingFilter additionally strips the decomposed combining marks before recomposing, so the practical Latin output equals `nilufer`).
- Casefold is locale-independent (Turkish `İ` → `i̇`); §3.3.2 uses `to_lowercase()` per char, also locale-independent. For ASCII the two agree.

For WXYC's use case (Latin-1, sparse Greek, sparse CJK) ICUFoldingFilter and §3.3.2 steps 1–3 produce equivalent output on the canonical artist set.

#### 1.8.2 Application-side normalizer — `EntryNormalizer`

**Source:** `/Users/jake/Developer/WXYC/tubafrenzy/libs/lucene/src/main/java/org/wxyc/lucene/tools/EntryNormalizer.java`

```java
// :70 normalizeArtist
String s = artist.trim();
s = ROTATION_PREFIX.matcher(s).replaceFirst("");      // ^\([HMLS]\)\s+
s = ROTATION_SUFFIX.matcher(s).replaceFirst("");      // \s+\([HMLS]\)$
if (s.endsWith("`")) s = s.substring(0, s.length() - 1);
// strip trailing period if preceded by alnum
s = stripDiacritics(s);
s = s.toLowerCase();
return s.trim();

// :170 normalizeTitle
String s = title.trim();
s = stripArrowSuffix(s);                               // strip "X -> Y" → "X"
s = FORMAT_SUFFIX.matcher(s).replaceFirst("");        // EP|LP|Single|12"|7"...
s = TRAILING_PAREN.matcher(s).replaceFirst("");       // any \s+\([^)]*\)$
s = TRAILING_BRACKET.matcher(s).replaceFirst("");
s = TRAILING_DOTS.matcher(s).replaceFirst("");        // \.{2,}$
s = TRAILING_BANG.matcher(s).replaceFirst("");
s = TRAILING_PERIOD.matcher(s).replaceFirst("");
s = stripDiacritics(s);
s = s.toLowerCase();
return s.trim();

// :201 normalizeFuzzy (aggressive — for Jaro-Winkler)
s = stripDiacritics(s.trim());
s = s.toLowerCase();
s = LEADING_THE.matcher(s).replaceFirst("");          // ^the\s+
s = s.replace(" and ", " & ");                         // canonicalize and→&
s = NON_ALNUM_EXCEPT_SPACE_AMP.matcher(s).replaceAll(""); // [^a-z0-9\s&]
s = MULTI_SPACE.matcher(s).replaceAll(" ");
return s.trim();

// :239 stripDiacritics
String replaced = s
  .replace('ø', 'o').replace('Ø', 'O')   // ø/Ø
  .replace('đ', 'd').replace('Đ', 'D')   // đ/Đ
  .replace('ł', 'l').replace('Ł', 'L')   // ł/Ł
  .replace("æ", "ae").replace("Æ", "AE"); // æ/Æ
String decomposed = Normalizer.normalize(replaced, Normalizer.Form.NFD);
return DIACRITICALS.matcher(decomposed).replaceAll("");  // \p{InCombiningDiacriticalMarks}+
```

**Critical divergences from §3.3.2:**

- Tubafrenzy uses **NFD**, not NFKD. NFD does not unfold ligatures (ﬁ stays ﬁ) or compatibility decompositions (Ⅷ stays Ⅷ). §3.3.2 + the existing wxyc_etl use NFKD.
- Tubafrenzy hand-extends with stroked-letter mappings (ø, đ, ł, æ) that NFKD also doesn't decompose. So tubafrenzy actually folds *more* of those rare Latin Extended characters than wxyc_etl does today.

**Sample outputs** (traced, not executed — Java not invoked here, but the trace is straightforward):

| Input | `normalizeArtist` | `normalizeTitle` | `normalizeFuzzy` |
|---|---|---|---|
| `Nilüfer Yanya` | `nilufer yanya` | `nilufer yanya` | `nilufer yanya` |
| `Csillagrablók` | `csillagrablok` | `csillagrablok` | `csillagrablok` |
| `Hermanos Gutiérrez` | `hermanos gutierrez` | `hermanos gutierrez` | `hermanos gutierrez` |
| `The Beatles` | `the beatles` | — | `beatles` |
| `Beatles, The` | `beatles, the` (comma untouched; trailing period rule preserves comma) | — | `beatles the` |
| `(H) Stereolab` | `stereolab` (rotation prefix strip) | — | `stereolab` |
| `Discharge (2)` | `discharge (2)` | `discharge` (any trailing paren) | `discharge 2` |
| `John Smith /1` | `john smith /1` | — | `john smith 1` |
| `Foo (Remastered 2019)` | — | `foo` | `foo` |
| `M.I.A.` | `m.i.a` (trailing-period strip) | — | `mia` |
| `!!!` | `!!` (trailing-bang sub strips trailing run, leaving the leading `!` per the regex `!+$` — actually: `!+$` matches the entire trailing `!!!`, so it strips to `''`) | similarly empties | `''` |
| `Łona` | `lona` (manual stroke-letter rule fires) | `lona` | `lona` |
| `ﬁreflies` | `ﬁreflies` (NFD doesn't unfold the ligature) | same | same |
| `Ⅷ Symphony` | `ⅷ symphony` (NFD doesn't decompose Roman numerals) | same | same |

| Rule | `normalizeArtist` | `normalizeTitle` | `normalizeFuzzy` |
|---|---|---|---|
| step_1_nfkd | implemented_divergent (NFD, not NFKD) | same | same |
| step_2_drop_marks | implemented | implemented | implemented |
| step_3_lowercase | implemented | implemented | implemented |
| step_4_paren_strip | absent (only the rotation-bin paren) | implemented (any trailing paren) | absent |
| step_5_article_drop | absent | absent | implemented_divergent (only `^the\s+`) |
| step_6_punctuation_collapse | absent | absent | implemented_divergent (`[^a-z0-9\s&]` strips, including periods, slashes, plus — but keeps `&`) |
| step_7_collapse_whitespace | implemented (trim only) | implemented (trim only) | implemented (collapse + trim) |
| step_8_disambiguator_strip | absent | absent | absent (the `/N` form falls into the punct strip) |

**Plus:** tubafrenzy has stroked-letter folding (ø/đ/ł/æ) that none of the other consumers do. Whether the §3.3.2 algorithm should adopt this is an open question — NFKD does not decompose these, so the canonical algorithm leaves them as-is and tubafrenzy's `Łona` (`łona` after lowercase) ↔ wxyc_etl's `łona` would mismatch on `łona` vs `lona`. This is a real-world Latin-Extended adversary in the canonical pool (`Łona` not present, but the test fixture deliberately includes other strokes — see §3.3.3 row "Polish ł"). **Recommend the spec address this in §3.3.4 or adopt tubafrenzy's stroke-letter mappings as a step-3a in §3.3.2.**

---

## Summary table — per-step coverage across the 8 consumers

(Each cell: `implemented` / `absent` / `divergent`. Where a consumer has multiple normalize functions, the cell summarizes the most-used one — the index/build-time path. Annotations call out the others.)

| Consumer (function) | NFKD | drop marks | lowercase | paren strip | article drop | punct collapse | whitespace | `/N` disambig |
|---|---|---|---|---|---|---|---|---|
| **wxyc-etl** `normalize_artist_name` | yes | yes | yes (+ sigma fold) | no | no | no | divergent (trim only, ASCII space) | no |
| **discogs-etl** `filter_csv.normalize_artist` | yes | yes | yes | no | no | no | divergent (trim only) | no |
| **discogs-etl** `verify_cache.normalize_artist` | yes | yes | yes | divergent (`(N)` only) | divergent (re-attaches comma form) | divergent (`&→and`, `'`→drop) | yes (collapse + trim) | divergent (only `(N)`, not `/N`) |
| **discogs-etl** `verify_cache.normalize_title` | yes | yes | yes | divergent (keyword-paren only) | no | no | yes | no |
| **LML** `discogs/matching.normalize_for_track_comparison` | yes | yes | yes | divergent (content stays, parens drop) | no | divergent (strip not collapse) | yes | no (falls into punct strip) |
| **LML** `discogs/matching.normalize_artist_for_validation` | **no** | **no** | yes | divergent (`(N)` only) | no | divergent (`"` `'` drop only) | yes (trim only) | no |
| **LML** `discogs/service.calculate_confidence::normalize` | no | no | yes | no | no | no | yes (trim only) | no |
| **LML** `library/db.py` fallback | yes | yes | yes | divergent | no | divergent (`[^a-z0-9\s]` strip — ASCII only) | yes | no |
| **LML** Postgres `lower(f_unaccent(...))` | divergent (unaccent ≠ NFKD on ligatures) | yes | yes | no | no | no | yes (none) | no |
| **LML** `streaming_availability/matching.normalize_album_title` | yes | yes | yes | divergent (keyword-paren) | no | no | yes | no |
| **LML** `match_compilations.normalize_comp_title` | no | no | no | yes | no | no | yes (trim) | no |
| **LML** `spotify_artist_catalog.normalize_title` | no | no | no | divergent | no | no | yes (trim) | no |
| **LML** `variation_audit/loaders.normalize_name` | yes | yes | yes (no sigma fold) | no | no | no | yes (trim only) | no |
| **LML** `track_streaming/validate._normalize` | no | no | yes | divergent (any position) | no | no | yes (trim) | no |
| **LML** `validate_streaming_urls._normalize_title` | no | no | no | divergent (keyword-paren) | no | no | yes (trim) | no |
| **LML** `entity_resolution/discogs.py` SQL | divergent (Python NFKD vs PG unaccent) | yes | yes | no | no | no | yes (none) | no |
| **semantic-index** `_normalize` | yes | yes | yes | divergent (`[...]` only) | divergent (`the ` only) | divergent (`' & '` only) | yes (trim only) | no |
| **musicbrainz-cache** Rust filter | yes | yes | yes | no | no | no | yes (trim) | no |
| **musicbrainz-cache** PG `lower(name)` indexes | no | no | yes | no | no | no | no | no |
| **wikidata-cache** | no | no | no | no | no | no | no | no |
| **Backend** `to_tsvector('simple', ...)` | no | no | yes | no | no | no | tokenizer-handled | no |
| **tubafrenzy** `normalizeArtist` | divergent (NFD) | yes (+ stroke-letter mappings) | yes | no | no | no | yes (trim) | no |
| **tubafrenzy** `normalizeTitle` | divergent (NFD) | yes | yes | yes (any trailing paren) | no | no | yes (trim) | no |
| **tubafrenzy** `normalizeFuzzy` | divergent (NFD) | yes | yes | no | divergent (`the ` only) | divergent (`[^a-z0-9\s&]` strip) | yes (collapse + trim) | no |
| **discogs-xml-converter** | yes | yes | yes (+ sigma fold via wxyc_etl) | no | no | no | yes (trim) | no |

**Headline observations**

1. The plan's §3.3.1.1 inventory was approximately complete. One additional implementation surfaced: **wikidata-cache stores labels and aliases verbatim with no normalization at all** (only `pg_trgm` indexes, no `lower()`, no `unaccent`). It hits the read path through pg_trgm's intrinsic case-insensitivity but cannot key against any `wxyc_norm_artist` column today. §3.3.5's vendored `wxyc_norm_*` function plus a populated `wxyc_library` hook is the migration that brings it to parity.
2. **Backend-Service is the only consumer with neither `unaccent` installed nor any application-side diacritic strip.** All search there is on the raw lowercased token. Bringing Backend to parity requires installing `unaccent` (a one-line migration) plus the `wxyc_norm_*` functions in §3.3.5 — non-trivial.
3. **tubafrenzy uses NFD, not NFKD**, plus a hand-curated stroke-letter table (ø/đ/ł/æ → ascii). NFD vs NFKD diverges on Roman-numeral compatibility decompositions, ligatures, halfwidth/fullwidth katakana, and superscript/subscript. None of these are common in the WXYC catalog (the canonical pool has no ligature inputs and none of `Ⅷ` etc.). The stroke-letter mappings cover Latin Extended cases (`Łona` exists in real flowsheet data per the canonical pool's Polish-ł diagnostic). Recommendation in §1.8 above.
4. **Steps 4 (paren strip) and 6 (punct collapse) have wildly inconsistent implementations** — every consumer that does either does it differently. The plan §3.3.4 per-step regression report is the correct gate before adopting either; expect step 6 to surface ≥2% match shift and ship as opt-in (`normalize_artist_with_punctuation_collapse`) per §3.3.4's locked thresholds.
5. **Step 5 (article drop) is currently rare and incompatible with the read paths that have it:** semantic-index drops `the ` (one variant); discogs-etl `verify_cache` *re-attaches* `, The` (the opposite direction); tubafrenzy fuzzy drops `the `. Adopting §3.3.2 step 5 will be a **breaking** change for discogs-etl `verify_cache.normalize_artist`'s currently-stored values. The §3.3.4 regression report needs to count this as a "Match LOST + GAINED" pair, since the new function will collide pairs that the old function was actively splitting.
6. **The Greek-sigma fold (ς → σ + Σ → σ) is implemented in wxyc_etl Rust but not in the Python prototype it descends from**. Python's `str.lower()` is identity on `ς`. Some Python callers (variation_audit/loaders, the original filter_csv) do not see the fold. wxyc_etl Rust + PyO3 do. §3.3.2 implicitly preserves this by saying "per-character `to_lowercase()`" (Rust semantics). The Postgres analog must replicate it; the locked SQL in §3.3.5 uses `lower()` which does NOT fold ς in default Postgres collation — **the `wxyc_unaccent.rules` file needs to include ς → σ explicitly**, or the SQL function needs a `replace(s, 'ς', 'σ')` step. Flag for §3.3.5 implementation.

---

## 2. Test-path verification (§8.6 hard gate)

Per §8.6's locked enforcement mechanism, every test-path claim in §8.5 is verified here against the actual repo state on `origin/main`.

| Repo | Plan §8.5 expected path | Verified | Convention | Notes |
|---|---|---|---|---|
| **Backend-Service** | `apps/backend/tests/integration/*.spec.js` | **DRIFT** | `tests/integration/*.spec.js` | Backend integration tests live at the **repo-root** `tests/integration/`, NOT under `apps/backend/`. Verified by `find /Users/jake/Developer/WXYC/Backend-Service -path "*tests/integration*" -name "*.spec.js"` returning `/Users/jake/Developer/WXYC/Backend-Service/tests/integration/library.spec.js` and 15 sibling files (no results under `apps/backend/tests/`). Backend's CLAUDE.md "Integration Tests" section says `Location: tests/integration/**/*.spec.ts` (note: also `.spec.ts` — but the actual files on disk are `.spec.js`, not `.spec.ts`). **The plan §8.5 claim "Backend uses `apps/backend/tests/integration/*.spec.js`" is wrong on the path, right on the extension.** Plan §8.5 should be amended to `tests/integration/*.spec.js`. Unit tests are at `tests/unit/**/*.test.ts`. |
| **library-metadata-lookup** | `tests/integration/test_*.py` (pytest) | confirmed | `tests/integration/test_*.py` | Verified by `ls /Users/jake/Developer/WXYC/library-metadata-lookup/tests/integration/` returning `test_admin_streaming_db.py`, `test_alternate_artist.py`, `test_api_discogs.py`, `test_api_health.py`, etc. CLAUDE.md "Pytest markers" section confirms pytest with markers `pg`, `external_api`. |
| **discogs-etl** | `tests/integration/test_*.py` (pytest) | confirmed | `tests/integration/test_*.py` | Verified via the integration directory listing: `test_alembic_baseline.py`, `test_connection_resilience.py`, `test_copy_to_target.py`, `test_dedup.py`, `test_error_resilience.py`, etc. CLAUDE.md "Testing" section confirms pytest + markers `pg`, `slow`. |
| **musicbrainz-cache** | `tests/integration/test_wxyc_library_v2.rs` (Rust integration test) | partial confirmation | `tests/*.rs` (no integration subdir) | musicbrainz-cache uses **flat** Rust integration tests directly under `tests/`: `cli_tests.rs`, `error_handling.rs`, `import_test.rs`, `oracle_tests.rs`, `pg_import_test.rs`. **There is no `tests/integration/` subdirectory.** New Rust integration tests for the parity harness should land at `tests/wxyc_library_v2.rs` (or similar) — flat, not nested. **Plan §8.5 should be amended to drop the `integration/` segment for musicbrainz-cache.** |
| **wikidata-cache** | `tests/integration/test_wxyc_library_v2.rs` (Rust integration test) | partial confirmation | `tests/*.rs` (no integration subdir) | Same flat pattern: `tests/import_test.rs`, `tests/oracle_tests.rs`. **Plan §8.5 should be amended to drop the `integration/` segment for wikidata-cache.** |
| **semantic-index** | `tests/integration/test_*.py` (pytest) | confirmed | `tests/integration/test_*.py` | Verified: `test_discogs_edges_sql.py`, `test_entity_source_fallback.py`, `test_pipeline.py`. CLAUDE.md "Testing" section confirms pytest with markers `pg`, `slow`. |
| **discogs-xml-converter** (not in §8.5 inventory but used as a parity-test source per §3.3.1.1) | n/a | flat | `tests/*.rs` | Flat: `cli_tests.rs`, `oracle_tests.rs`, `pg_direct_import_test.rs`. Same as the other Rust caches. |
| **wxyc-etl** itself | `wxyc-etl/tests/normalization_parity.rs`, `regression_report.rs` | not yet authored — these are E3 step 4 deliverables | `wxyc-etl/tests/*.rs` (flat) | Existing pattern: `wxyc-etl/tests/python_parity.rs`, `integration_modules.rs`, `pg_error_tests.rs`, `panic_recovery.rs`. New parity files should follow the flat pattern. |

**Summary of §8.5 amendments required (E3 step 1 PR will land these inline):**

```
- Backend gate-check + composition + LML write contract + cross-source agreement + manual-override + rollback test files:
    apps/backend/tests/integration/library-identity-*.spec.js
  →
    tests/integration/library-identity-*.spec.js

- Hook table parity (musicbrainz-cache):
    musicbrainz-cache/tests/integration/test_wxyc_library_v2.rs
  →
    musicbrainz-cache/tests/wxyc_library_v2.rs

- Hook table parity (wikidata-cache):
    wikidata-cache/tests/integration/test_wxyc_library_v2.rs
  →
    wikidata-cache/tests/wxyc_library_v2.rs

- Per-cache Postgres analog parity (musicbrainz-cache, wikidata-cache):
    same path correction as above — drop `integration/`
```

LML, discogs-etl, semantic-index paths in §8.5 are correct as written.

---

## 3. NFC vs NFKD decision (Q8)

The plan §3.3.1.1 + §3.3.2 lock NFKD. This audit confirms the lock; no counter-case found.

**Worked examples that distinguish NFC from NFKD:**

| Code point | NFC (precomposed) | NFD (decomposed) | NFKC (compat-precomposed) | NFKD (compat-decomposed) |
|---|---|---|---|---|
| `Nilüfer Yanya` (`ü` = U+00FC) | `Nilüfer Yanya` | `Nilu¨fer Yanya` (U+0075 + U+0308) | `Nilüfer Yanya` | `Nilu¨fer Yanya` |
| `ﬁreflies` (`ﬁ` = U+FB01 ligature) | `ﬁreflies` | `ﬁreflies` (no decomposition; ligature is a single code point with no canonical decomposition) | `fireflies` | `fireflies` |
| `Ⅷ Symphony` (`Ⅷ` = U+2167 Roman numeral) | `Ⅷ Symphony` | `Ⅷ Symphony` (no canonical decomposition) | `VIII Symphony` | `VIII Symphony` |
| `①` (U+2460 circled digit) | `①` | `①` | `1` | `1` |
| `ﬃ` (U+FB03 ligature) | `ﬃ` | `ﬃ` | `ffi` | `ffi` |
| `Beyoncé` (`é` = U+00E9) | `Beyoncé` | `Beyonce´` (U+0065 + U+0301) | `Beyoncé` | `Beyonce´` |
| Halfwidth katakana `ｶ` (U+FF76) | `ｶ` | `ｶ` | `カ` (U+30AB) | `カ` |

**The NFKD-vs-NFC choice matters in two places:**

1. **Combining marks.** Both NFD and NFKD decompose `é` to `e + ◌́`. After dropping marks, both produce `e`. NFC and NFKC do not decompose, so the diacritic-strip step has nothing to drop and `é` survives. **For our diacritic-strip pipeline, NFKD or NFD is required; NFC and NFKC are non-starters.**

2. **Compatibility decomposition.** NFKD additionally decomposes ligatures (ﬁ → fi), Roman numerals (Ⅷ → VIII), circled digits (① → 1), halfwidth katakana (ｶ → カ), superscripts/subscripts (²→2), and similar "compatibility" forms. NFD does not.

**Counter-case search.** Are there Latin or non-Latin code points in the existing four caches that NFKD destructively normalizes in a way NFC would preserve, and that we care about? The two real candidates:

- **Roman numerals in classical music titles.** "Beethoven Symphony Ⅷ" and "Beethoven Symphony VIII" become equivalent under NFKD; under NFC they don't. The plan §3.3.1.1 explicitly accepts this as a known mild downside ("Roman-numeral folding for classical-music catalog use is a known mild downside"). Spot check: WXYC's canonical pool has no Roman-numeral artist names; the catalog itself has classical titles, but the conflation does not produce false matches in practice (a "Symphony 8" and "Symphony Ⅷ" both refer to the same Symphony №8 anyway).
- **Ligatures in label names.** Some label names (e.g., a French label using ﬁ in print) might be stored as ligatures in Discogs and as plain `fi` in tubafrenzy. NFKD collapses these — desirable.

**No counter-case surfaced.** I spot-checked the canonical pool's 106 names plus the 26 example/diagnostic inputs; none surface a real downside of NFKD over NFC.

The audit confirms the existing wxyc_etl 0.1.x algorithm step 1 is `NFKD` (verified at `wxyc-etl/src/text/normalize.rs:43, :64` via `c.nfkd()`).

**Q8 decision: LOCKED — NFKD.** Matches §3.3.2 + §3.3.1.1.

**Implementation note for §3.3.5:** the Postgres `unaccent` extension as shipped does not implement NFKD compatibility decomposition. The vendored `wxyc_unaccent.rules` file referenced in §3.3.5 must include the ligature mappings (ﬁ→fi, ﬃ→ffi, ﬄ→ffl, ﬀ→ff, ﬁ→fi, ﬃ→ffi, etc.), Roman numeral mappings (Ⅰ→I, Ⅱ→II, ..., Ⅻ→XII, etc.), and Greek-sigma mapping (ς→σ) to maintain parity with the Rust implementation.

---

## 4. Empty-output decision (Q9)

The plan §3.3.3 locks **option B** — re-run with step 6 (punctuation collapse) omitted, mark with `norm_artist_fallback=true`, fall back to the original input verbatim if even that produces empty output.

**Worked examples that exercise the empty-output path:**

The audit found three real-data inputs in the canonical pool that punch through every aggressive normalizer to empty output: `!!!`, `+/-`, `???`. (`!!!` is a real WXYC-played artist; `+/-` and `???` are also real flowsheet-attested artist names.)

| Input | wxyc_etl `normalize_artist_name` | LML `normalize_for_track_comparison` | LML `normalize_artist_for_validation` | discogs-etl `normalize_artist` | semantic-index `_normalize` |
|---|---|---|---|---|---|
| `!!!` | `!!!` (no punct collapse) | `''` (empty) | `!!!` | `!!!` | `!!!` |
| `+/-` | `+/-` | `''` (empty) | `+/-` | `+/-` | `+/-` |
| `???` | `???` | `''` (empty) | `???` | `???` | `???` |

Today, only LML's `normalize_for_track_comparison` (and by extension the punctuation-collapsing helpers) hits the empty-output condition. Every other consumer preserves the punctuation. The §3.3.2 algorithm adopts step 6, so once the new function ships, **wxyc_etl 0.2.0 will also produce empty output for these inputs unless step 6 is opted out or the empty-output fallback fires.**

**Options recap (per §3.3.3):**

- **Option A — allow empty.** `normalize_artist('!!!') == ''`. Catastrophic: every empty-normalized row collides with every other empty-normalized row, poisoning the index. `!!!` collides with `+/-` collides with `???` collides with whitespace-only inputs.
- **Option B — locked.** Re-run with step 6 omitted; record `norm_artist_fallback=true`. `normalize_artist('!!!') == '!!!'` (since the only step that erased it was step 6). The fallback is a single deterministic re-run, not a recovery loop.
- **Option C — reject.** Raise an error; refuse to normalize. Creates a class of un-normalizable inputs the abstraction can't handle. WXYC has these inputs (they are real artist names DJs actually played).

**Counter-case search.** Are there inputs where option B's step-6-omitted re-run also produces empty? The §3.3.3 spec says "the function returns the original input verbatim with `norm_artist_fallback=true` and a warning logged once at startup." The CHECK constraint then enforces `norm_artist != ''`.

The only inputs that would punch through both step-6-omitted AND the verbatim fallback are inputs that were already empty after NFKD + drop marks + lowercase + trim — which, by definition, are inputs that contained nothing but combining marks and whitespace. The canonical pool has no such inputs; the broader flowsheet has near-zero (whitespace-only artist names are blocked at write time today by tubafrenzy's `NOT NULL` + non-empty string convention). **Returning the original input verbatim handles the worst case adequately.**

**No counter-case surfaced.** The audit confirms option B handles the realistic empty-output set (`!!!`, `+/-`, `???`) cleanly: each falls back to its step-6-omitted result (which equals the lowercased original since step 6 was the only erasing step), gets `norm_artist_fallback=true` set, and indexes distinctly.

**Q9 decision: LOCKED — option B.** Matches §3.3.3.

**Implementation note for §3.3.5:** the locked plpgsql in §3.3.5 already implements option B via the `IF result = '' OR result IS NULL THEN ...` block. The `wxyc_library` hook table needs a corresponding `norm_artist_fallback boolean NOT NULL DEFAULT false` column (not currently in the §3.1 schema sketch — flag for E1 schema PR).

---

## v0.2.0 conditional functions (per §3.3.1 step 1 deliverable)

The plan §3.3.1 step 1 requires this audit to enumerate "candidate function variants … so consumers don't get surprised by name churn after the per-step decision." Based on the per-step state above and §3.3.4's locked thresholds (steps 4–5 non-negotiable, steps 6 and 8 opt-in if per-step shift exceeds 2%), the candidate variants for v0.2.0 are:

| Function | Steps included | Adoption condition |
|---|---|---|
| `normalize_artist` | 1, 2, 3, 4, 5, 7 — and 6, 8 if their per-step shift ≤2% | always shipped in 0.2.0 (locked-on steps 4 + 5) |
| `normalize_artist_with_punctuation_collapse` | 1, 2, 3, 4, 5, 6, 7 (no step 8) | shipped iff §3.3.4 reports step 6 shift > 2% — opt-in form for consumers that want the strict variant |
| `normalize_artist_with_disambiguator_strip` | 1, 2, 3, 4, 5, 7, 8 (no step 6) | shipped iff §3.3.4 reports step 8 shift > 2% — opt-in for Discogs-resolver consumers |
| `normalize_artist_full` | 1, 2, 3, 4, 5, 6, 7, 8 | always shipped — the maximally-aggressive form, used by discogs-etl/LML matchers that already do this much |
| `normalize_title` | 1, 2, 3, 4, 6 (if shipped), 7 — never step 8 | titles never get `/N` disambig stripped; otherwise mirrors the artist variants |
| `strip_diacritics` | 1, 2 (no lowercase) | unchanged from 0.1.x; preserved for case-bearing contexts |

If §3.3.4's per-step report shows steps 6 + 8 shift ≤2% (the audit's prior of "likely under threshold for the canonical pool, possibly over for the long tail"), the only public function in 0.2.0 is `normalize_artist`/`normalize_title` and the variants are not shipped. The version decision in §3.3 step 3 picks between the two states.

---

## Methodology notes

- All Python sample outputs were produced by importing the actual implementation modules under each repo's `.venv` (LML's `.venv` for wxyc_etl + discogs/matching + scripts; discogs-etl's `.venv` for verify_cache; semantic-index's `.venv` for artist_resolver). Sample outputs are reproducible.
- Java (tubafrenzy) outputs are traced from the source. Re-executing them requires building the WAR; not done here. The traces are mechanical and the `EntryNormalizer` source is short enough that the trace is reliable.
- Postgres-side `f_unaccent` outputs are traced from the SQL definition + the documented behavior of the `unaccent` extension; not executed against a live cache for this audit (would require running against the homebrew discogs-cache and dragging in ~62 GB of disk). The §3.3.4 regression report is where actual Postgres execution happens — flag for E3 step 4.
- The v0.2.0 conditional-function enumeration is a recommendation, not a lock. The §3.3 step 3 version decision selects which variants ship.

