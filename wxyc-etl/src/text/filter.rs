//! HashSet-based artist and title filtering.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

use super::normalize::{normalize_artist_name, normalize_title};

/// Title filter backed by a normalized HashSet.
///
/// Matches release titles against a set of known titles. Supports exact match
/// and bracket-suffix stripping (Discogs titles often include catalog info
/// like `[Sublime Frequencies: SF044]`).
pub struct TitleFilter {
    titles: HashSet<String>,
}

impl TitleFilter {
    /// Load titles from a file (one per line) and normalize them.
    pub fn from_file(path: &Path) -> anyhow::Result<Self> {
        let content = fs::read_to_string(path)?;
        let titles = content
            .lines()
            .map(|line| line.trim())
            .filter(|line| !line.is_empty())
            .map(normalize_title)
            .collect();
        Ok(TitleFilter { titles })
    }

    /// Check if a release title matches any title in the filter.
    ///
    /// Tries exact match first, then strips trailing `[...]` suffixes
    /// (common in Discogs for catalog numbers and reissue notes).
    pub fn matches(&self, title: &str) -> bool {
        let normalized = normalize_title(title);
        if self.titles.contains(&normalized) {
            return true;
        }

        // Strip trailing bracket suffix: "Title [Remastered]" -> "Title"
        if let Some(bracket_pos) = normalized.rfind('[') {
            let stripped = normalized[..bracket_pos].trim_end();
            if !stripped.is_empty() && self.titles.contains(stripped) {
                return true;
            }
        }

        false
    }

    /// Number of titles in the filter set.
    pub fn len(&self) -> usize {
        self.titles.len()
    }

    /// Whether the filter set is empty.
    pub fn is_empty(&self) -> bool {
        self.titles.is_empty()
    }
}

/// Artist filter backed by a normalized HashSet, with optional alias support.
///
/// When aliases are loaded (from `artist_alias.csv`), the filter checks both
/// canonical artist names and their aliases/name-variations by artist_id.
pub struct ArtistFilter {
    artists: HashSet<String>,
    /// artist_id -> set of normalized alias names (includes name variations)
    aliases: HashMap<u64, Vec<String>>,
}

impl ArtistFilter {
    /// Load artist names from a file (one per line) and normalize them.
    pub fn from_file(path: &Path) -> anyhow::Result<Self> {
        let content = fs::read_to_string(path)?;
        let artists = content
            .lines()
            .map(|line| line.trim())
            .filter(|line| !line.is_empty())
            .map(normalize_artist_name)
            .collect();
        Ok(ArtistFilter {
            artists,
            aliases: HashMap::new(),
        })
    }

    /// Load artist aliases from `artist_alias.csv`.
    ///
    /// Builds a lookup from artist_id to normalized alias names. When combined
    /// with `matches_any_with_ids()`, this enables matching releases where the
    /// credited artist is known by a different name in the library.
    pub fn load_aliases(&mut self, csv_path: &Path) -> anyhow::Result<usize> {
        let mut rdr = csv::Reader::from_path(csv_path)?;
        let mut count = 0;

        for result in rdr.records() {
            let record = result?;
            let artist_id: u64 = record[0].parse().unwrap_or(0);
            let alias_name = &record[2]; // alias_name column

            let normalized = normalize_artist_name(alias_name);
            if !normalized.is_empty() {
                self.aliases.entry(artist_id).or_default().push(normalized);
                count += 1;
            }
        }

        Ok(count)
    }

    /// Check if any of the given artist names match the filter.
    pub fn matches_any<'a, I>(&self, names: I) -> bool
    where
        I: IntoIterator<Item = &'a str>,
    {
        names
            .into_iter()
            .any(|name| self.artists.contains(&normalize_artist_name(name)))
    }

    /// Check if any artist matches by canonical name or by alias lookup.
    ///
    /// For each (artist_id, name) pair:
    /// 1. Check the canonical name against the library set
    /// 2. Look up aliases by artist_id and check each against the library set
    pub fn matches_any_with_ids(&self, artists: &[(u64, &str)]) -> bool {
        for (artist_id, name) in artists {
            if self.artists.contains(&normalize_artist_name(name)) {
                return true;
            }
            if let Some(alias_names) = self.aliases.get(artist_id) {
                for alias in alias_names {
                    if self.artists.contains(alias) {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Whether alias data has been loaded.
    pub fn has_aliases(&self) -> bool {
        !self.aliases.is_empty()
    }

    /// Number of artists in the filter set.
    pub fn len(&self) -> usize {
        self.artists.len()
    }

    /// Whether the filter set is empty.
    pub fn is_empty(&self) -> bool {
        self.artists.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    // --- TitleFilter ---

    #[test]
    fn test_title_filter_from_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("titles.txt");
        fs::write(&path, "Sugar Hill\nOK Computer\n  Café Tacvba  \n\n").unwrap();

        let filter = TitleFilter::from_file(&path).unwrap();
        assert_eq!(filter.len(), 3);
        assert!(filter.matches("Sugar Hill"));
        assert!(filter.matches("ok computer"));
        assert!(filter.matches("Café Tacvba"));
        assert!(!filter.matches("Unknown Album"));
    }

    #[test]
    fn test_title_filter_exact_match() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("titles.txt");
        fs::write(&path, "Sugar Hill\n").unwrap();

        let filter = TitleFilter::from_file(&path).unwrap();
        assert!(filter.matches("Sugar Hill"));
        assert!(filter.matches("sugar hill"));
        assert!(filter.matches("SUGAR HILL"));
        assert!(!filter.matches("Sugar Hill Records"));
    }

    #[test]
    fn test_title_filter_bracket_suffix_stripping() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("titles.txt");
        fs::write(&path, "Sublime Frequencies\nSugar Hill\n").unwrap();

        let filter = TitleFilter::from_file(&path).unwrap();
        assert!(filter.matches("Sublime Frequencies"));
        assert!(filter.matches("Sublime Frequencies [SF044]"));
        assert!(filter.matches("Sugar Hill [Remastered]"));
        assert!(!filter.matches("Unknown Album [Deluxe]"));
    }

    #[test]
    fn test_title_filter_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("titles.txt");
        fs::write(&path, "\n\n").unwrap();

        let filter = TitleFilter::from_file(&path).unwrap();
        assert!(filter.is_empty());
        assert!(!filter.matches("Anything"));
    }

    // --- ArtistFilter ---

    #[test]
    fn test_artist_filter_from_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("artists.txt");
        fs::write(&path, "Stereolab\nNilüfer Yanya\n  Yo La Tengo  \n\n").unwrap();

        let filter = ArtistFilter::from_file(&path).unwrap();
        assert_eq!(filter.len(), 3);
        assert!(filter.matches_any(["Stereolab"].iter().copied()));
        assert!(filter.matches_any(["Nilüfer Yanya"].iter().copied()));
        assert!(filter.matches_any(["yo la tengo"].iter().copied()));
        assert!(!filter.matches_any(["Unknown Artist"].iter().copied()));
    }

    #[test]
    fn test_artist_filter_matches_any() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("artists.txt");
        fs::write(&path, "Stereolab\nNilüfer Yanya\n").unwrap();

        let filter = ArtistFilter::from_file(&path).unwrap();
        assert!(filter.matches_any(["Unknown", "Stereolab"].iter().copied()));
        assert!(!filter.matches_any(["Unknown", "Other"].iter().copied()));
    }

    #[test]
    fn test_load_aliases_and_match() {
        let dir = tempfile::tempdir().unwrap();

        let lib_path = dir.path().join("artists.txt");
        fs::write(&lib_path, "Madlib\n").unwrap();

        let alias_path = dir.path().join("artist_alias.csv");
        fs::write(
            &alias_path,
            "artist_id,artist_name,alias_name\n\
             123,Madlib,Quasimoto\n\
             123,Madlib,Madlib\n\
             123,Madlib,Lord Quas\n\
             123,Madlib,DJ Rels\n",
        )
        .unwrap();

        let mut filter = ArtistFilter::from_file(&lib_path).unwrap();
        let count = filter.load_aliases(&alias_path).unwrap();
        assert_eq!(count, 4);
        assert!(filter.has_aliases());

        // "P. Diddy" doesn't match directly, but alias lookup finds "Puff Daddy"
        assert!(!filter.matches_any(["Quasimoto"].iter().copied()));
        assert!(filter.matches_any_with_ids(&[(123, "Quasimoto")]));

        // Unknown artist doesn't match
        assert!(!filter.matches_any_with_ids(&[(999, "Unknown")]));
    }

    #[test]
    fn test_matches_any_with_ids_canonical_name_still_works() {
        let dir = tempfile::tempdir().unwrap();
        let lib_path = dir.path().join("artists.txt");
        fs::write(&lib_path, "Stereolab\n").unwrap();

        let filter = ArtistFilter::from_file(&lib_path).unwrap();
        assert!(filter.matches_any_with_ids(&[(300, "Stereolab")]));
        assert!(!filter.matches_any_with_ids(&[(300, "Unknown")]));
    }
}
