//! Module interaction tests for wxyc-etl.
//!
//! These tests wire two or more modules together to verify cross-module
//! contracts. Each module has its own unit tests; these integration tests
//! exercise the boundaries between modules using realistic WXYC data.

use std::collections::HashSet;

use wxyc_etl::csv_writer::{CsvFileSpec, MultiCsvWriter};
use wxyc_etl::fuzzy::{
    batch_classify_releases, batch_fuzzy_resolve, classify_release, Classification, ClassifyConfig,
    LibraryIndex,
};
use wxyc_etl::import::{ColumnMapping, ImportDedupSet};
use wxyc_etl::pipeline::*;
use wxyc_etl::state::{PipelineState, STEP_NAMES};
use wxyc_etl::text::{batch_normalize, normalize_artist_name};

// ---------------------------------------------------------------------------
// WXYC example data (from wxyc-shared canonical fixtures)
// ---------------------------------------------------------------------------

/// (artist, title, label) tuples from real WXYC flowsheets.
fn wxyc_releases() -> Vec<(&'static str, &'static str, &'static str)> {
    vec![
        ("Autechre", "Confield", "Warp"),
        ("Prince Jammy", "...Destroys The Space Invaders", "Greensleeves"),
        ("Juana Molina", "DOGA", "Sonamos"),
        ("Stereolab", "Aluminum Tunes", "Duophonic"),
        ("Cat Power", "Moon Pix", "Matador Records"),
        ("Jessica Pratt", "On Your Own Love Again", "Drag City"),
        ("Chuquimamani-Condori", "Edits", "self-released"),
        ("Duke Ellington & John Coltrane", "Duke Ellington & John Coltrane", "Impulse Records"),
        ("Sessa", "Pequena Vertigem de Amor", "Mexican Summer"),
        ("Anne Gillis", "Eyry", "Art into Life"),
        ("Father John Misty", "I Love You, Honeybear", "Sub Pop"),
        ("Rafael Toral", "Traveling Light", "Drag City"),
        ("Buck Meek", "Gasoline", "4AD"),
        ("Nourished by Time", "The Passionate Ones", "XL"),
        ("For Tracy Hyde", "Hotel Insomnia", "P-Vine Records"),
        ("Rochelle Jordan", "Through the Wall", "EMPIRE"),
        ("Large Professor", "1st Class", "Matador Records"),
    ]
}

/// Library (artist, title) pairs for building the fuzzy index.
fn library_pairs() -> Vec<(String, String)> {
    wxyc_releases()
        .iter()
        .map(|(a, t, _)| (a.to_string(), t.to_string()))
        .collect()
}

// ===========================================================================
// 1. Pipeline scanner -> CSV writer
// ===========================================================================

mod scanner_to_csv_writer {
    use super::*;

    /// A PipelineOutput adapter that writes (artist, title, label) rows to a
    /// MultiCsvWriter's release.csv file.
    struct CsvOutput {
        writer: MultiCsvWriter,
    }

    impl CsvOutput {
        fn new(dir: &std::path::Path) -> Self {
            let specs = vec![CsvFileSpec::new(
                "release.csv",
                &["artist", "title", "label"],
            )];
            Self {
                writer: MultiCsvWriter::new(dir, &specs).unwrap(),
            }
        }
    }

    impl PipelineOutput<(String, String, String)> for CsvOutput {
        fn write_item(&mut self, item: &(String, String, String)) -> anyhow::Result<()> {
            self.writer
                .writer(0)
                .write_record(&[&item.0, &item.1, &item.2])?;
            Ok(())
        }

        fn flush(&mut self) -> anyhow::Result<()> {
            self.writer.flush_all()
        }

        fn finish(&mut self) -> anyhow::Result<()> {
            self.writer.flush_all()
        }
    }

    /// Scan byte batches, process through the pipeline, write CSV output.
    /// Verify headers and content match expected WXYC releases.
    #[test]
    fn byte_batches_through_processor_to_csv() {
        let dir = tempfile::tempdir().unwrap();
        let releases = wxyc_releases();
        let expected_count = releases.len();

        // Build CSV lines as raw bytes (simulating a scanner reading a file)
        let csv_lines: Vec<String> = releases
            .iter()
            .map(|(a, t, l)| format!("{}\t{}\t{}", a, t, l))
            .collect();

        let config = BatchConfig {
            batch_size: 5,
            channel_capacity: 4,
        };
        let (rx, handle) = start_byte_scanner(
            move |tx| {
                for chunk in csv_lines.chunks(5) {
                    let mut batch = ByteBatch::new();
                    for line in chunk {
                        batch.push_slice(line.as_bytes());
                    }
                    tx.send(batch)?;
                }
                Ok(expected_count)
            },
            config,
        );

        let mut output = CsvOutput::new(dir.path());

        // Transform: parse tab-separated bytes into (artist, title, label) tuple
        let no_dedup: Option<DedupConfig<'_, fn(&[u8]) -> Option<u64>>> = None;
        let stats = run_byte_pipeline(
            rx,
            handle,
            |bytes| {
                let s = std::str::from_utf8(bytes).ok()?;
                let mut parts = s.splitn(3, '\t');
                let artist = parts.next()?.to_string();
                let title = parts.next()?.to_string();
                let label = parts.next()?.to_string();
                Some((artist, title, label))
            },
            &mut output,
            no_dedup,
        )
        .unwrap();
        output.finish().unwrap();

        assert_eq!(stats.scanned, expected_count);
        assert_eq!(stats.written, expected_count);
        assert_eq!(stats.filtered, 0);

        // Read back the CSV and verify
        let csv_path = dir.path().join("release.csv");
        let mut rdr = csv::Reader::from_path(&csv_path).unwrap();

        // Verify headers
        let headers = rdr.headers().unwrap();
        assert_eq!(headers.get(0).unwrap(), "artist");
        assert_eq!(headers.get(1).unwrap(), "title");
        assert_eq!(headers.get(2).unwrap(), "label");

        // Verify content
        let records: Vec<csv::StringRecord> = rdr.records().map(|r| r.unwrap()).collect();
        assert_eq!(records.len(), expected_count);

        // First record should be Autechre
        assert_eq!(&records[0][0], "Autechre");
        assert_eq!(&records[0][1], "Confield");
        assert_eq!(&records[0][2], "Warp");

        // Last record should be Large Professor
        let last = &records[expected_count - 1];
        assert_eq!(&last[0], "Large Professor");
        assert_eq!(&last[1], "1st Class");
        assert_eq!(&last[2], "Matador Records");
    }

    /// Verify that dedup across multiple byte batches works when writing to CSV.
    #[test]
    fn dedup_across_batches_in_csv_output() {
        let dir = tempfile::tempdir().unwrap();

        let config = BatchConfig {
            batch_size: 10,
            channel_capacity: 4,
        };
        let (rx, handle) = start_byte_scanner(
            |tx| {
                // Batch 1: Autechre and Stereolab
                tx.send(ByteBatch::from_slices(&[
                    b"1\tAutechre\tConfield",
                    b"2\tStereolab\tAluminum Tunes",
                ]))?;
                // Batch 2: duplicate Autechre (same ID 1) and new Cat Power
                tx.send(ByteBatch::from_slices(&[
                    b"1\tAutechre\tConfield",
                    b"3\tCat Power\tMoon Pix",
                ]))?;
                Ok(4)
            },
            config,
        );

        let specs = vec![CsvFileSpec::new("release.csv", &["id", "artist", "title"])];
        let mut csv_writer = MultiCsvWriter::new(dir.path(), &specs).unwrap();

        struct CsvIdOutput<'a> {
            writer: &'a mut MultiCsvWriter,
        }
        impl PipelineOutput<(String, String, String)> for CsvIdOutput<'_> {
            fn write_item(&mut self, item: &(String, String, String)) -> anyhow::Result<()> {
                self.writer
                    .writer(0)
                    .write_record(&[&item.0, &item.1, &item.2])?;
                Ok(())
            }
            fn flush(&mut self) -> anyhow::Result<()> {
                self.writer.flush_all()
            }
            fn finish(&mut self) -> anyhow::Result<()> {
                self.writer.flush_all()
            }
        }

        let mut output = CsvIdOutput {
            writer: &mut csv_writer,
        };
        let mut seen = HashSet::new();
        let stats = run_byte_pipeline(
            rx,
            handle,
            |bytes| {
                let s = std::str::from_utf8(bytes).ok()?;
                let mut parts = s.splitn(3, '\t');
                let id = parts.next()?.to_string();
                let artist = parts.next()?.to_string();
                let title = parts.next()?.to_string();
                Some((id, artist, title))
            },
            &mut output,
            Some(DedupConfig {
                seen_ids: &mut seen,
                id_fn: |bytes| {
                    let s = std::str::from_utf8(bytes).ok()?;
                    let id_str = s.split('\t').next()?;
                    id_str.parse().ok()
                },
            }),
        )
        .unwrap();
        output.finish().unwrap();

        assert_eq!(stats.scanned, 4);
        assert_eq!(stats.written, 3);
        assert_eq!(stats.duplicates, 1);

        // Verify the CSV has exactly 3 data rows (no duplicate Autechre)
        let mut rdr = csv::Reader::from_path(dir.path().join("release.csv")).unwrap();
        let records: Vec<csv::StringRecord> = rdr.records().map(|r| r.unwrap()).collect();
        assert_eq!(records.len(), 3);

        let artists: Vec<&str> = records.iter().map(|r| &r[1]).collect();
        assert_eq!(artists, vec!["Autechre", "Stereolab", "Cat Power"]);
    }

    /// Multi-file CSV writer: scanner produces data for two related tables.
    #[test]
    fn scanner_to_multi_file_csv() {
        let dir = tempfile::tempdir().unwrap();

        let config = BatchConfig {
            batch_size: 4,
            channel_capacity: 4,
        };
        let releases = wxyc_releases();
        let release_count = releases.len();

        let (rx, handle) = start_scanner(
            move |tx| {
                for (i, (artist, title, label)) in releases.iter().enumerate() {
                    tx.send_item((i as u64, artist.to_string(), title.to_string(), label.to_string()))?;
                }
                Ok(release_count)
            },
            config,
        );

        let specs = vec![
            CsvFileSpec::new("release.csv", &["id", "title", "label"]),
            CsvFileSpec::new("release_artist.csv", &["release_id", "artist_name"]),
        ];
        let mut csv_writer = MultiCsvWriter::new(dir.path(), &specs).unwrap();

        struct MultiFileOutput<'a> {
            writer: &'a mut MultiCsvWriter,
        }
        impl PipelineOutput<(u64, String, String, String)> for MultiFileOutput<'_> {
            fn write_item(&mut self, item: &(u64, String, String, String)) -> anyhow::Result<()> {
                let id_str = item.0.to_string();
                self.writer
                    .writer_by_name("release.csv")
                    .unwrap()
                    .write_record(&[&id_str, &item.2, &item.3])?;
                self.writer
                    .writer_by_name("release_artist.csv")
                    .unwrap()
                    .write_record(&[&id_str, &item.1])?;
                Ok(())
            }
            fn flush(&mut self) -> anyhow::Result<()> {
                self.writer.flush_all()
            }
            fn finish(&mut self) -> anyhow::Result<()> {
                self.writer.flush_all()
            }
        }

        let mut output = MultiFileOutput {
            writer: &mut csv_writer,
        };
        let stats = run_pipeline(rx, handle, |item| Some(item.clone()), &mut output).unwrap();
        output.finish().unwrap();

        assert_eq!(stats.scanned, release_count);
        assert_eq!(stats.written, release_count);

        // Verify release.csv
        let mut rdr = csv::Reader::from_path(dir.path().join("release.csv")).unwrap();
        let release_records: Vec<csv::StringRecord> = rdr.records().map(|r| r.unwrap()).collect();
        assert_eq!(release_records.len(), release_count);
        assert_eq!(&release_records[0][1], "Confield"); // Autechre's album

        // Verify release_artist.csv
        let mut rdr = csv::Reader::from_path(dir.path().join("release_artist.csv")).unwrap();
        let artist_records: Vec<csv::StringRecord> = rdr.records().map(|r| r.unwrap()).collect();
        assert_eq!(artist_records.len(), release_count);
        assert_eq!(&artist_records[0][1], "Autechre");

        // Verify referential integrity: every release_id in release_artist.csv
        // exists in release.csv
        let release_ids: HashSet<String> = release_records
            .iter()
            .map(|r| r[0].to_string())
            .collect();
        for record in &artist_records {
            assert!(
                release_ids.contains(&record[0]),
                "release_artist.csv references release_id {} not in release.csv",
                &record[0]
            );
        }
    }
}

// ===========================================================================
// 2. Text normalization -> fuzzy matching
// ===========================================================================

mod text_normalization_to_fuzzy {
    use super::*;

    /// Normalize WXYC artist names, then classify against a library index.
    /// Verify that normalization (diacritics stripping, lowercasing) does not
    /// break fuzzy matching — exact matches should still classify as KEEP.
    #[test]
    fn normalized_names_classify_as_keep() {
        let pairs = library_pairs();
        let index = LibraryIndex::from_pairs(&pairs);
        let config = ClassifyConfig::default();

        for (artist, title, _label) in wxyc_releases() {
            let norm_artist = normalize_artist_name(artist);
            let norm_title = normalize_artist_name(title);
            let result = classify_release(&norm_artist, &norm_title, &index, &config);
            assert_eq!(
                result,
                Classification::Keep,
                "Expected KEEP for ({:?}, {:?}) -> ({:?}, {:?})",
                artist, title, norm_artist, norm_title,
            );
        }
    }

    /// batch_normalize followed by batch_classify_releases should produce
    /// the same results as individual normalize + classify calls.
    #[test]
    fn batch_normalize_then_batch_classify_consistency() {
        let pairs = library_pairs();
        let index = LibraryIndex::from_pairs(&pairs);
        let config = ClassifyConfig::default();

        let artists: Vec<String> = wxyc_releases().iter().map(|(a, _, _)| a.to_string()).collect();
        let titles: Vec<String> = wxyc_releases().iter().map(|(_, t, _)| t.to_string()).collect();

        // Batch path: normalize then classify
        let norm_artists = batch_normalize(&artists);
        let norm_titles = batch_normalize(&titles);
        let batch_results =
            batch_classify_releases(&norm_artists, &norm_titles, &index, &config);

        // Individual path
        let individual_results: Vec<Classification> = artists
            .iter()
            .zip(titles.iter())
            .map(|(a, t)| {
                let na = normalize_artist_name(a);
                let nt = normalize_artist_name(t);
                classify_release(&na, &nt, &index, &config)
            })
            .collect();

        assert_eq!(batch_results, individual_results);
    }

    /// Verify normalization doesn't create empty strings that break matching.
    /// ASCII-only names should normalize to non-empty strings.
    #[test]
    fn normalization_preserves_nonempty_for_ascii_artists() {
        let artists: Vec<String> = wxyc_releases().iter().map(|(a, _, _)| a.to_string()).collect();
        let normalized = batch_normalize(&artists);

        for (original, normed) in artists.iter().zip(normalized.iter()) {
            assert!(
                !normed.is_empty(),
                "normalize_artist_name({:?}) produced empty string",
                original,
            );
        }
    }

    /// Fuzzy resolve with normalized names: verify that normalized WXYC artist
    /// names resolve back to their catalog entries.
    #[test]
    fn normalized_names_resolve_against_catalog() {
        let catalog: Vec<String> = wxyc_releases()
            .iter()
            .map(|(a, _, _)| a.to_string())
            .collect();

        // Normalize each name, then resolve against the original catalog
        let normalized: Vec<String> = catalog.iter().map(|a| normalize_artist_name(a)).collect();
        let results = batch_fuzzy_resolve(&normalized, &catalog, 0.7, 2, 0.02);

        for (i, (result, original)) in results.iter().zip(catalog.iter()).enumerate() {
            assert!(
                result.is_some(),
                "Failed to resolve normalized name {:?} (original: {:?}) at index {}",
                &normalized[i], original, i,
            );
        }
    }

    /// Normalize names with diacritics, then classify. The diacritics-stripped
    /// form should still match the index (which also stores normalized forms).
    #[test]
    fn diacritics_stripping_does_not_reduce_fuzzy_accuracy() {
        // Artists with non-ASCII characters
        let test_cases = [
            ("Juana Molina", "DOGA"),
            ("Chuquimamani-Condori", "Edits"),
            ("Sessa", "Pequena Vertigem de Amor"),
        ];
        let pairs = library_pairs();
        let index = LibraryIndex::from_pairs(&pairs);
        let config = ClassifyConfig::default();

        for (artist, title) in &test_cases {
            let norm_artist = normalize_artist_name(artist);
            let norm_title = normalize_artist_name(title);
            let result = classify_release(&norm_artist, &norm_title, &index, &config);
            assert_eq!(
                result,
                Classification::Keep,
                "Diacritics stripping broke matching for ({:?}, {:?})",
                artist, title,
            );
        }
    }

    /// Non-library releases should not classify as KEEP even after normalization.
    #[test]
    fn non_library_releases_classified_correctly_after_normalization() {
        let pairs = library_pairs();
        let index = LibraryIndex::from_pairs(&pairs);
        let config = ClassifyConfig::default();

        // These are NOT in the library
        let non_library = [
            ("Fennesz", "Endless Summer"),
            ("Grouper", "Dragging a Dead Deer Up a Hill"),
            ("Pan Sonic", "Aaltopiiri"),
        ];

        for (artist, title) in &non_library {
            let norm_artist = normalize_artist_name(artist);
            let norm_title = normalize_artist_name(title);
            let result = classify_release(&norm_artist, &norm_title, &index, &config);
            assert_ne!(
                result,
                Classification::Keep,
                "Non-library release ({:?}, {:?}) should not be KEEP",
                artist, title,
            );
        }
    }
}

// ===========================================================================
// 3. State -> import: track import steps, resume after partial failure
// ===========================================================================

mod state_to_import {
    use super::*;

    /// Create a PipelineState, track import steps using ColumnMapping metadata,
    /// verify that completed steps are preserved and failed steps can be resumed.
    #[test]
    fn state_tracks_import_steps_with_column_mappings() {
        let dir = tempfile::tempdir().unwrap();
        let state_path = dir.path().join("state.json");

        // Create mappings for two tables
        let release_mapping = ColumnMapping::new(
            vec!["id".into(), "title".into(), "label".into()],
            vec!["id".into(), "title".into(), "label_name".into()],
            vec!["id".into()],
            Some(vec!["id".into()]),
        );
        let artist_mapping = ColumnMapping::new(
            vec!["release_id".into(), "artist_name".into()],
            vec!["release_id".into(), "artist_name".into()],
            vec!["release_id".into()],
            Some(vec!["release_id".into(), "artist_name".into()]),
        );

        // Simulate: create_schema and import_csv succeed
        let mut state = PipelineState::new("postgresql:///discogs_test", "/tmp/csv", STEP_NAMES);
        state.mark_completed("create_schema");
        state.mark_completed("import_csv");
        state.save(&state_path).unwrap();

        // Verify the mappings are valid alongside state
        assert_eq!(release_mapping.source_columns.len(), 3);
        assert_eq!(release_mapping.db_columns[2], "label_name");
        assert_eq!(artist_mapping.unique_key_indices().unwrap(), vec![0, 1]);

        // Simulate: create_indexes fails
        state.mark_failed("create_indexes", "duplicate key violation");
        state.save(&state_path).unwrap();

        // Load and verify resume state
        let loaded = PipelineState::load(&state_path).unwrap();
        assert!(loaded.is_completed("create_schema"));
        assert!(loaded.is_completed("import_csv"));
        assert_eq!(loaded.step_status("create_indexes"), "failed");
        assert_eq!(
            loaded.step_error("create_indexes"),
            Some("duplicate key violation")
        );
        assert!(!loaded.is_completed("dedup"));
    }

    /// Simulate a partial import failure mid-way through dedup, then resume.
    /// The resume should skip completed steps and retry failed ones.
    #[test]
    fn resume_after_partial_import_failure() {
        let dir = tempfile::tempdir().unwrap();
        let state_path = dir.path().join("state.json");

        // First run: complete some steps, fail on import_tracks
        let mut state =
            PipelineState::new("postgresql:///discogs_test", "/tmp/wxyc_csv", STEP_NAMES);
        let steps_to_complete = ["create_schema", "import_csv", "create_indexes", "dedup"];
        for step in &steps_to_complete {
            state.mark_completed(step);
        }
        state.mark_failed("import_tracks", "connection lost");
        state.save(&state_path).unwrap();

        // Simulate resume: load state, skip completed steps
        let mut resumed = PipelineState::load(&state_path).unwrap();
        resumed
            .validate_resume("postgresql:///discogs_test", "/tmp/wxyc_csv")
            .unwrap();

        // Verify completed steps are still completed
        for step in &steps_to_complete {
            assert!(
                resumed.is_completed(step),
                "Step {:?} should be completed on resume",
                step
            );
        }

        // The failed step should be retried
        assert_eq!(resumed.step_status("import_tracks"), "failed");
        assert!(!resumed.is_completed("import_tracks"));

        // Simulate: retry the failed step, now it succeeds
        resumed.mark_completed("import_tracks");
        resumed.mark_completed("create_track_indexes");
        resumed.mark_completed("prune");
        resumed.mark_completed("vacuum");
        resumed.mark_completed("set_logged");
        resumed.save(&state_path).unwrap();

        // Verify all steps now completed
        let final_state = PipelineState::load(&state_path).unwrap();
        for step in STEP_NAMES {
            assert!(
                final_state.is_completed(step),
                "Step {:?} should be completed after resume",
                step
            );
        }
    }

    /// ImportDedupSet works correctly with ColumnMapping's unique_key_indices
    /// to deduplicate rows during import.
    #[test]
    fn dedup_set_with_column_mapping_unique_keys() {
        let mapping = ColumnMapping::new(
            vec![
                "release_id".into(),
                "artist_name".into(),
                "extra".into(),
            ],
            vec![
                "release_id".into(),
                "artist_name".into(),
                "extra".into(),
            ],
            vec!["release_id".into()],
            Some(vec!["release_id".into(), "artist_name".into()]),
        );

        let key_indices = mapping.unique_key_indices().unwrap();
        assert_eq!(key_indices, vec![0, 1]);

        let mut dedup = ImportDedupSet::new();

        // Simulate importing rows with dedup
        let rows = vec![
            vec!["1", "Autechre", "extra1"],
            vec!["2", "Stereolab", "extra2"],
            vec!["1", "Autechre", "extra3"], // duplicate by (release_id, artist_name)
            vec!["1", "Cat Power", "extra4"], // same release_id, different artist — not a dup
            vec!["3", "Juana Molina", "extra5"],
        ];

        let mut written = 0;
        let mut duplicates = 0;

        for row in &rows {
            let key: Vec<&str> = key_indices.iter().map(|&i| row[i]).collect();
            if dedup.insert(&key) {
                written += 1;
            } else {
                duplicates += 1;
            }
        }

        assert_eq!(written, 4);
        assert_eq!(duplicates, 1);
        assert_eq!(dedup.len(), 4);
    }

    /// Validate that resume rejects mismatched database URLs.
    #[test]
    fn resume_rejects_mismatched_config() {
        let dir = tempfile::tempdir().unwrap();
        let state_path = dir.path().join("state.json");

        let state = PipelineState::new("postgresql:///discogs_prod", "/data/csv", STEP_NAMES);
        state.save(&state_path).unwrap();

        let loaded = PipelineState::load(&state_path).unwrap();

        // Wrong database URL
        assert!(loaded
            .validate_resume("postgresql:///discogs_test", "/data/csv")
            .is_err());

        // Wrong CSV directory
        assert!(loaded
            .validate_resume("postgresql:///discogs_prod", "/other/csv")
            .is_err());

        // Correct config
        assert!(loaded
            .validate_resume("postgresql:///discogs_prod", "/data/csv")
            .is_ok());
    }
}

// ===========================================================================
// 4. Full pipeline end-to-end: scanner -> processor (with filter) -> writer
// ===========================================================================

mod full_pipeline_e2e {
    use super::*;

    /// A struct representing a parsed release, used throughout the full pipeline.
    #[derive(Clone, Debug, PartialEq)]
    struct Release {
        id: u64,
        artist: String,
        title: String,
        label: String,
    }

    /// PipelineOutput that collects releases in a Vec for verification.
    struct VecOutput {
        items: Vec<Release>,
    }

    impl VecOutput {
        fn new() -> Self {
            Self { items: Vec::new() }
        }
    }

    impl PipelineOutput<Release> for VecOutput {
        fn write_item(&mut self, item: &Release) -> anyhow::Result<()> {
            self.items.push(item.clone());
            Ok(())
        }
        fn flush(&mut self) -> anyhow::Result<()> {
            Ok(())
        }
        fn finish(&mut self) -> anyhow::Result<()> {
            Ok(())
        }
    }

    /// Full pipeline: scanner -> processor (with label filter) -> writer.
    /// Uses WXYC release data and verifies deterministic output ordering.
    #[test]
    fn scanner_processor_filter_writer_deterministic() {
        let releases = wxyc_releases();
        let total = releases.len();

        // Filter: only keep releases on specific labels
        let keep_labels: HashSet<&str> =
            ["Warp", "Drag City", "Matador Records", "Sub Pop"].into_iter().collect();

        let config = BatchConfig {
            batch_size: 4,
            channel_capacity: 4,
        };

        // Scanner: emit releases as typed items
        let releases_clone = releases.clone();
        let (rx, handle) = start_scanner(
            move |tx| {
                for (i, (artist, title, label)) in releases_clone.iter().enumerate() {
                    tx.send_item(Release {
                        id: i as u64,
                        artist: artist.to_string(),
                        title: title.to_string(),
                        label: label.to_string(),
                    })?;
                }
                Ok(total)
            },
            config,
        );

        // Processor + filter: keep only releases on selected labels
        let mut output = VecOutput::new();
        let stats = run_pipeline(
            rx,
            handle,
            |release| {
                if keep_labels.contains(release.label.as_str()) {
                    Some(release.clone())
                } else {
                    None
                }
            },
            &mut output,
        )
        .unwrap();

        assert_eq!(stats.scanned, total);

        // Count expected: Warp(Autechre), Drag City(Jessica Pratt, Rafael Toral),
        // Matador Records(Cat Power, Large Professor), Sub Pop(Father John Misty)
        let expected_artists = [
            "Autechre",
            "Cat Power",
            "Jessica Pratt",
            "Father John Misty",
            "Rafael Toral",
            "Large Professor",
        ];
        assert_eq!(stats.written, expected_artists.len());
        assert_eq!(stats.filtered, total - expected_artists.len());

        // Verify output contains exactly the expected artists (order preserved)
        let output_artists: Vec<&str> = output.items.iter().map(|r| r.artist.as_str()).collect();
        for expected in &expected_artists {
            assert!(
                output_artists.contains(expected),
                "Expected {:?} in output, got {:?}",
                expected, output_artists,
            );
        }

        // Verify order preservation: output should be in the same order as the
        // input, just with filtered items removed
        let expected_order = [
            "Autechre",
            "Cat Power",
            "Jessica Pratt",
            "Father John Misty",
            "Rafael Toral",
            "Large Professor",
        ];
        assert_eq!(
            output_artists, expected_order,
            "Output order should match input order (minus filtered items)",
        );
    }

    /// Full pipeline with dedup: byte scanner -> processor -> CSV writer.
    /// Duplicate release IDs across batches should be deduplicated.
    #[test]
    fn full_pipeline_with_dedup_to_csv() {
        let dir = tempfile::tempdir().unwrap();

        let config = BatchConfig {
            batch_size: 10,
            channel_capacity: 4,
        };

        // Build input with intentional duplicates
        let (rx, handle) = start_byte_scanner(
            |tx| {
                tx.send(ByteBatch::from_slices(&[
                    b"1\tAutechre\tConfield\tWarp",
                    b"2\tStereolab\tAluminum Tunes\tDuophonic",
                    b"3\tCat Power\tMoon Pix\tMatador Records",
                ]))?;
                tx.send(ByteBatch::from_slices(&[
                    b"1\tAutechre\tConfield\tWarp",        // duplicate
                    b"4\tJuana Molina\tDOGA\tSonamos",
                    b"2\tStereolab\tAluminum Tunes\tDuophonic", // duplicate
                    b"5\tJessica Pratt\tOn Your Own Love Again\tDrag City",
                ]))?;
                Ok(7)
            },
            config,
        );

        let specs = vec![CsvFileSpec::new(
            "release.csv",
            &["id", "artist", "title", "label"],
        )];
        let mut csv_writer = MultiCsvWriter::new(dir.path(), &specs).unwrap();

        struct CsvReleaseOutput<'a> {
            writer: &'a mut MultiCsvWriter,
        }
        impl PipelineOutput<(String, String, String, String)> for CsvReleaseOutput<'_> {
            fn write_item(
                &mut self,
                item: &(String, String, String, String),
            ) -> anyhow::Result<()> {
                self.writer
                    .writer(0)
                    .write_record(&[&item.0, &item.1, &item.2, &item.3])?;
                Ok(())
            }
            fn flush(&mut self) -> anyhow::Result<()> {
                self.writer.flush_all()
            }
            fn finish(&mut self) -> anyhow::Result<()> {
                self.writer.flush_all()
            }
        }

        let mut output = CsvReleaseOutput {
            writer: &mut csv_writer,
        };
        let mut seen = HashSet::new();
        let stats = run_byte_pipeline(
            rx,
            handle,
            |bytes| {
                let s = std::str::from_utf8(bytes).ok()?;
                let mut parts = s.splitn(4, '\t');
                let id = parts.next()?.to_string();
                let artist = parts.next()?.to_string();
                let title = parts.next()?.to_string();
                let label = parts.next()?.to_string();
                Some((id, artist, title, label))
            },
            &mut output,
            Some(DedupConfig {
                seen_ids: &mut seen,
                id_fn: |bytes| {
                    let s = std::str::from_utf8(bytes).ok()?;
                    let id_str = s.split('\t').next()?;
                    id_str.parse().ok()
                },
            }),
        )
        .unwrap();
        output.finish().unwrap();

        assert_eq!(stats.scanned, 7);
        assert_eq!(stats.written, 5);
        assert_eq!(stats.duplicates, 2);

        // Read back CSV and verify
        let mut rdr = csv::Reader::from_path(dir.path().join("release.csv")).unwrap();
        let records: Vec<csv::StringRecord> = rdr.records().map(|r| r.unwrap()).collect();
        assert_eq!(records.len(), 5);

        // Verify no duplicate IDs in output
        let ids: Vec<&str> = records.iter().map(|r| &r[0]).collect();
        let unique_ids: HashSet<&str> = ids.iter().copied().collect();
        assert_eq!(ids.len(), unique_ids.len(), "CSV should contain no duplicate IDs");

        // Verify order is preserved
        assert_eq!(ids, vec!["1", "2", "3", "4", "5"]);
    }

    /// Pipeline with normalization in the processor: normalize artist names
    /// during processing, then write normalized output.
    #[test]
    fn pipeline_with_normalization_in_processor() {
        let releases = wxyc_releases();
        let total = releases.len();

        let config = BatchConfig {
            batch_size: 6,
            channel_capacity: 4,
        };

        let releases_clone = releases.clone();
        let (rx, handle) = start_scanner(
            move |tx| {
                for (artist, title, label) in releases_clone.iter() {
                    tx.send_item((artist.to_string(), title.to_string(), label.to_string()))?;
                }
                Ok(total)
            },
            config,
        );

        struct NormalizedOutput {
            items: Vec<(String, String)>,
        }
        impl PipelineOutput<(String, String)> for NormalizedOutput {
            fn write_item(&mut self, item: &(String, String)) -> anyhow::Result<()> {
                self.items.push(item.clone());
                Ok(())
            }
            fn flush(&mut self) -> anyhow::Result<()> {
                Ok(())
            }
            fn finish(&mut self) -> anyhow::Result<()> {
                Ok(())
            }
        }

        let mut output = NormalizedOutput { items: Vec::new() };
        let stats = run_pipeline(
            rx,
            handle,
            |item: &(String, String, String)| {
                let norm_artist = normalize_artist_name(&item.0);
                let norm_title = normalize_artist_name(&item.1);
                Some((norm_artist, norm_title))
            },
            &mut output,
        )
        .unwrap();

        assert_eq!(stats.scanned, total);
        assert_eq!(stats.written, total);

        // Verify all items are normalized (lowercase, no leading/trailing whitespace)
        for (artist, title) in &output.items {
            assert_eq!(
                artist, &artist.to_lowercase().trim().to_string(),
                "Artist {:?} not normalized",
                artist,
            );
            assert_eq!(
                title, &title.to_lowercase().trim().to_string(),
                "Title {:?} not normalized",
                title,
            );
        }

        // Spot-check specific normalizations
        assert_eq!(output.items[0].0, "autechre");
        assert_eq!(output.items[0].1, "confield");
        assert_eq!(output.items[3].0, "stereolab");
        assert_eq!(output.items[3].1, "aluminum tunes");
    }

    /// Determinism: running the same pipeline twice produces identical output.
    #[test]
    fn pipeline_output_is_deterministic() {
        fn run_once() -> Vec<String> {
            let releases = wxyc_releases();
            let total = releases.len();

            let config = BatchConfig {
                batch_size: 3,
                channel_capacity: 4,
            };

            let releases_clone = releases.clone();
            let (rx, handle) = start_scanner(
                move |tx| {
                    for (artist, title, _label) in releases_clone.iter() {
                        tx.send_item(format!("{} - {}", artist, title))?;
                    }
                    Ok(total)
                },
                config,
            );

            struct CollectOutput {
                items: Vec<String>,
            }
            impl PipelineOutput<String> for CollectOutput {
                fn write_item(&mut self, item: &String) -> anyhow::Result<()> {
                    self.items.push(item.clone());
                    Ok(())
                }
                fn flush(&mut self) -> anyhow::Result<()> {
                    Ok(())
                }
                fn finish(&mut self) -> anyhow::Result<()> {
                    Ok(())
                }
            }

            let mut output = CollectOutput { items: Vec::new() };
            run_pipeline(rx, handle, |s| Some(s.clone()), &mut output).unwrap();
            output.items
        }

        let run1 = run_once();
        let run2 = run_once();
        assert_eq!(run1, run2, "Pipeline output should be deterministic across runs");
        assert_eq!(run1.len(), wxyc_releases().len());
        assert_eq!(run1[0], "Autechre - Confield");
    }
}
