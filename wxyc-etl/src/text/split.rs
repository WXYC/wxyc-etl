//! Multi-artist name decomposition.
//!
//! Ports `discogs-cache/lib/artist_splitting.py` to Rust.
//! Splits combined multi-artist library entries into individual components.

use std::collections::HashSet;

use super::normalize::normalize_artist_name;

/// Split a combined artist name into individual components.
///
/// Returns `Some(components)` when splitting succeeds, `None` if the name
/// doesn't appear to be a multi-artist entry.
///
/// Splits on `, ` (comma-space), ` / ` (slash), and ` + ` (plus).
/// Does NOT split on ` & ` or ` and ` — those are handled by
/// [`split_artist_name_contextual`] which can check against known artists.
///
/// Comma guard: skips the split entirely if any component after splitting is
/// purely numeric. This prevents splitting "10,000 Maniacs".
pub fn split_artist_name(name: &str) -> Option<Vec<String>> {
    let name = name.trim();
    if name.is_empty() {
        return None;
    }

    let components = try_comma_split(name)
        .or_else(|| try_delimiter_split(name, " / "))
        .or_else(|| try_delimiter_split(name, " + "))?;

    let result = dedupe_valid(components);

    if result.is_empty() || (result.len() == 1 && !name.contains(", ") && !name.contains(" / ") && !name.contains(" + ")) {
        None
    } else {
        Some(result)
    }
}

/// Context-aware artist name splitting.
///
/// First applies all context-free splits from [`split_artist_name`]. Then,
/// for remaining unsplit components (or the original name if no context-free
/// split applied), tries splitting on ` & ` when at least one resulting
/// component (after normalization) exists in `known_artists`.
///
/// The `known_artists` set should contain normalized artist names (lowercase,
/// accent-stripped) from the full library.
pub fn split_artist_name_contextual(
    name: &str,
    known_artists: &HashSet<String>,
) -> Option<Vec<String>> {
    let components = split_artist_name(name);

    if let Some(components) = components {
        // Re-check each component for contextual & splitting
        let mut expanded = Vec::new();
        for c in &components {
            if let Some(sub) = try_ampersand_split(c, known_artists) {
                expanded.extend(sub);
            } else {
                expanded.push(c.clone());
            }
        }
        let deduped = dedupe(expanded);
        Some(deduped)
    } else {
        // No context-free split applied; try ampersand split on the whole name
        let name = name.trim();
        try_ampersand_split(name, known_artists)
    }
}

/// Try splitting on ` & ` if at least one component is a known artist.
fn try_ampersand_split(name: &str, known_artists: &HashSet<String>) -> Option<Vec<String>> {
    if !name.contains(" & ") {
        return None;
    }

    let parts: Vec<&str> = name.split(" & ").map(|p| p.trim()).collect();
    if parts.len() < 2 {
        return None;
    }

    // Check if any component is a known artist
    let any_known = parts.iter().any(|p| {
        known_artists.contains(&normalize_artist_name(p))
    });

    if !any_known {
        return None;
    }

    let valid: Vec<String> = parts
        .into_iter()
        .filter(|p| valid_component(p))
        .map(String::from)
        .collect();

    if valid.len() >= 2 { Some(valid) } else { None }
}

/// Try comma-based splitting with trailing-and handling and numeric guard.
fn try_comma_split(name: &str) -> Option<Vec<&str>> {
    if !name.contains(", ") {
        return None;
    }

    let mut parts: Vec<&str> = name.split(", ").map(|p| p.trim()).collect();
    if parts.len() < 2 {
        return None;
    }

    // Numeric guard: block split if any component is purely numeric
    if parts.iter().any(|p| is_numeric(p.trim())) {
        return None;
    }

    // Handle trailing "and X" in the last component
    parts = split_trailing_and(parts);

    if parts.len() >= 2 {
        Some(parts)
    } else {
        None
    }
}

/// Try splitting on a simple delimiter (` / ` or ` + `).
fn try_delimiter_split<'a>(name: &'a str, delimiter: &str) -> Option<Vec<&'a str>> {
    if !name.contains(delimiter) {
        return None;
    }

    let parts: Vec<&str> = name.split(delimiter).map(|p| p.trim()).collect();
    if parts.len() >= 2 { Some(parts) } else { None }
}

/// Handle trailing "and X" in the last comma-split component.
///
/// "Emerson, Lake, and Palmer" -> ["Emerson", "Lake", "Palmer"]
fn split_trailing_and(mut components: Vec<&str>) -> Vec<&str> {
    if components.len() < 2 {
        return components;
    }

    let last = components.last().unwrap().trim();
    if let Some(rest) = last
        .strip_prefix("and ")
        .or_else(|| last.strip_prefix("And "))
        .or_else(|| last.strip_prefix("AND "))
    {
        let len = components.len();
        components[len - 1] = rest.trim();
    }

    components
}

/// Check if a string is purely numeric (digits, possibly with commas).
fn is_numeric(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_digit() || c == ',')
}

/// Check if a split component is meaningful (more than 1 character).
fn valid_component(s: &str) -> bool {
    s.trim().len() > 1
}

/// Filter invalid components and deduplicate, preserving order.
fn dedupe_valid(components: Vec<&str>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut result = Vec::new();
    for c in components {
        let trimmed = c.trim();
        if valid_component(trimmed) && seen.insert(trimmed.to_string()) {
            result.push(trimmed.to_string());
        }
    }
    result
}

/// Deduplicate while preserving order.
fn dedupe(items: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut result = Vec::new();
    for item in items {
        if seen.insert(item.clone()) {
            result.push(item);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    // --- split_artist_name (context-free) ---

    #[test]
    fn test_comma_split() {
        assert_eq!(
            split_artist_name("Mike Vainio, Ryoji, Alva Noto"),
            Some(vec!["Mike Vainio".into(), "Ryoji".into(), "Alva Noto".into()])
        );
    }

    #[test]
    fn test_plus_split() {
        assert_eq!(
            split_artist_name("Mika Vainio + Ryoji Ikeda + Alva Noto"),
            Some(vec!["Mika Vainio".into(), "Ryoji Ikeda".into(), "Alva Noto".into()])
        );
    }

    #[test]
    fn test_slash_split() {
        assert_eq!(
            split_artist_name("J Dilla / Jay Dee"),
            Some(vec!["J Dilla".into(), "Jay Dee".into()])
        );
    }

    #[test]
    fn test_plus_deduplicates() {
        assert_eq!(
            split_artist_name("David + David"),
            Some(vec!["David".into()])
        );
    }

    #[test]
    fn test_numeric_comma_guard() {
        assert_eq!(split_artist_name("10,000 Maniacs"), None);
    }

    #[test]
    fn test_trailing_and_stripped() {
        assert_eq!(
            split_artist_name("Emerson, Lake, and Palmer"),
            Some(vec!["Emerson".into(), "Lake".into(), "Palmer".into()])
        );
    }

    #[test]
    fn test_trailing_ampersand_kept_without_context() {
        assert_eq!(
            split_artist_name("Crosby, Stills, Nash & Young"),
            Some(vec!["Crosby".into(), "Stills".into(), "Nash & Young".into()])
        );
    }

    #[test]
    fn test_no_split_on_and() {
        assert_eq!(split_artist_name("Andy Human and the Reptoids"), None);
    }

    #[test]
    fn test_no_split_on_with() {
        assert_eq!(split_artist_name("Nurse with Wound"), None);
    }

    #[test]
    fn test_single_char_filtered() {
        assert_eq!(split_artist_name("A + B"), None);
    }

    #[test]
    fn test_no_delimiter() {
        assert_eq!(split_artist_name("Autechre"), None);
    }

    #[test]
    fn test_empty_string() {
        assert_eq!(split_artist_name(""), None);
    }

    #[test]
    fn test_whitespace_trimmed() {
        assert_eq!(
            split_artist_name("  Cat Power  /  Liz Phair  "),
            Some(vec!["Cat Power".into(), "Liz Phair".into()])
        );
    }

    #[test]
    fn test_does_not_split_ampersand_alone() {
        assert_eq!(split_artist_name("Duke Ellington & John Coltrane"), None);
    }

    #[test]
    fn test_comma_with_short_numeric_component() {
        assert_eq!(split_artist_name("808,303"), None);
    }

    #[test]
    fn test_fred_hopkins_slash() {
        assert_eq!(
            split_artist_name("Fred Hopkins / Dierdre Murray"),
            Some(vec!["Fred Hopkins".into(), "Dierdre Murray".into()])
        );
    }

    #[test]
    fn test_1000_homo_djs() {
        assert_eq!(split_artist_name("1,000 Homo DJs"), None);
    }

    // --- split_artist_name_contextual ---

    #[test]
    fn test_contextual_ampersand_with_known_artist() {
        let known: HashSet<String> = ["duke ellington"].iter().map(|s| s.to_string()).collect();
        assert_eq!(
            split_artist_name_contextual("Duke Ellington & John Coltrane", &known),
            Some(vec!["Duke Ellington".into(), "John Coltrane".into()])
        );
    }

    #[test]
    fn test_contextual_ampersand_without_known_artist() {
        let known: HashSet<String> = HashSet::new();
        assert_eq!(
            split_artist_name_contextual("Simon & Garfunkel", &known),
            None
        );
    }

    #[test]
    fn test_contextual_ampersand_second_component_known() {
        let known: HashSet<String> = ["john coltrane"].iter().map(|s| s.to_string()).collect();
        assert_eq!(
            split_artist_name_contextual("Duke Ellington & John Coltrane", &known),
            Some(vec!["Duke Ellington".into(), "John Coltrane".into()])
        );
    }

    #[test]
    fn test_contextual_context_free_splits_still_work() {
        let known: HashSet<String> = HashSet::new();
        assert_eq!(
            split_artist_name_contextual("J Dilla / Jay Dee", &known),
            Some(vec!["J Dilla".into(), "Jay Dee".into()])
        );
    }

    #[test]
    fn test_contextual_13_and_god() {
        let known: HashSet<String> = ["god"].iter().map(|s| s.to_string()).collect();
        assert_eq!(
            split_artist_name_contextual("13 & God", &known),
            Some(vec!["13".into(), "God".into()])
        );
    }

    #[test]
    fn test_contextual_known_artists_normalized() {
        let known: HashSet<String> = ["bjork"].iter().map(|s| s.to_string()).collect();
        assert_eq!(
            split_artist_name_contextual("Björk & Thom Yorke", &known),
            Some(vec!["Björk".into(), "Thom Yorke".into()])
        );
    }

    #[test]
    fn test_contextual_mixed_comma_and_ampersand() {
        let known: HashSet<String> = ["young"].iter().map(|s| s.to_string()).collect();
        assert_eq!(
            split_artist_name_contextual("Crosby, Stills, Nash & Young", &known),
            Some(vec!["Crosby".into(), "Stills".into(), "Nash".into(), "Young".into()])
        );
    }

    #[test]
    fn test_contextual_mixed_comma_and_ampersand_no_known() {
        let known: HashSet<String> = HashSet::new();
        assert_eq!(
            split_artist_name_contextual("Crosby, Stills, Nash & Young", &known),
            Some(vec!["Crosby".into(), "Stills".into(), "Nash & Young".into()])
        );
    }

    #[test]
    fn test_contextual_band_names_not_split_sly() {
        let known: HashSet<String> = ["sly"].iter().map(|s| s.to_string()).collect();
        assert_eq!(
            split_artist_name_contextual("Sly and the Family Stone", &known),
            None
        );
    }

    #[test]
    fn test_contextual_band_names_not_split_nurse() {
        let known: HashSet<String> = ["nurse"].iter().map(|s| s.to_string()).collect();
        assert_eq!(
            split_artist_name_contextual("Nurse with Wound", &known),
            None
        );
    }
}
