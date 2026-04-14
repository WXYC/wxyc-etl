//! Multi-file CSV writer for ETL pipelines.
//!
//! Manages N `csv::Writer` instances (one per output file), each with
//! pre-written headers. Ported from the per-repo writer patterns in
//! `discogs-xml-converter/src/writer.rs` and `wikidata-json-filter/src/writer.rs`.

mod writer;

pub use writer::{CsvFileSpec, MultiCsvWriter};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_files_with_headers() {
        let dir = tempfile::tempdir().unwrap();
        let specs = vec![
            CsvFileSpec::new("release.csv", &["id", "title", "country"]),
            CsvFileSpec::new("release_artist.csv", &["release_id", "artist_name"]),
        ];
        let writer = MultiCsvWriter::new(dir.path(), &specs).unwrap();
        drop(writer);

        let content = std::fs::read_to_string(dir.path().join("release.csv")).unwrap();
        assert!(content.starts_with("id,title,country\n"));

        let content2 = std::fs::read_to_string(dir.path().join("release_artist.csv")).unwrap();
        assert!(content2.starts_with("release_id,artist_name\n"));
    }

    #[test]
    fn creates_output_directory_if_missing() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("sub").join("dir");
        let specs = vec![CsvFileSpec::new("test.csv", &["a"])];
        let writer = MultiCsvWriter::new(&nested, &specs).unwrap();
        assert!(nested.exists());
        drop(writer);
    }

    #[test]
    fn write_and_flush() {
        let dir = tempfile::tempdir().unwrap();
        let specs = vec![CsvFileSpec::new("test.csv", &["a", "b"])];
        let mut writer = MultiCsvWriter::new(dir.path(), &specs).unwrap();
        writer.writer(0).write_record(&["1", "hello"]).unwrap();
        writer.flush_all().unwrap();

        let mut rdr = csv::Reader::from_path(dir.path().join("test.csv")).unwrap();
        let records: Vec<_> = rdr.records().map(|r| r.unwrap()).collect();
        assert_eq!(records.len(), 1);
        assert_eq!(&records[0][0], "1");
        assert_eq!(&records[0][1], "hello");
    }

    #[test]
    fn writer_by_name() {
        let dir = tempfile::tempdir().unwrap();
        let specs = vec![
            CsvFileSpec::new("a.csv", &["x"]),
            CsvFileSpec::new("b.csv", &["y"]),
        ];
        let mut writer = MultiCsvWriter::new(dir.path(), &specs).unwrap();
        writer
            .writer_by_name("b.csv")
            .unwrap()
            .write_record(&["42"])
            .unwrap();
        writer.flush_all().unwrap();

        let content = std::fs::read_to_string(dir.path().join("b.csv")).unwrap();
        assert!(content.contains("42"));
        assert!(writer.writer_by_name("nonexistent.csv").is_none());
    }

    #[test]
    fn output_dir_accessor() {
        let dir = tempfile::tempdir().unwrap();
        let specs = vec![CsvFileSpec::new("test.csv", &["a"])];
        let writer = MultiCsvWriter::new(dir.path(), &specs).unwrap();
        assert_eq!(writer.output_dir(), dir.path());
    }

    // --- Integration tests: realistic multi-file CSV output ---

    #[test]
    fn write_wxyc_release_data_to_multiple_files() {
        let dir = tempfile::tempdir().unwrap();
        let specs = vec![
            CsvFileSpec::new("release.csv", &["release_id", "title", "country"]),
            CsvFileSpec::new("release_artist.csv", &["release_id", "artist_name", "role"]),
        ];
        let mut writer = MultiCsvWriter::new(dir.path(), &specs).unwrap();

        // Write release rows
        writer.writer(0).write_record(&["1001", "Confield", "UK"]).unwrap();
        writer.writer(0).write_record(&["1002", "Aluminum Tunes", "UK"]).unwrap();
        writer.writer(0).write_record(&["1003", "Moon Pix", "US"]).unwrap();
        writer.writer(0).write_record(&["1004", "DOGA", "AR"]).unwrap();

        // Write release_artist rows
        writer.writer(1).write_record(&["1001", "Autechre", "Main"]).unwrap();
        writer.writer(1).write_record(&["1002", "Stereolab", "Main"]).unwrap();
        writer.writer(1).write_record(&["1003", "Cat Power", "Main"]).unwrap();
        writer.writer(1).write_record(&["1004", "Juana Molina", "Main"]).unwrap();

        writer.flush_all().unwrap();

        // Verify release.csv content
        let mut rdr = csv::Reader::from_path(dir.path().join("release.csv")).unwrap();
        let records: Vec<csv::StringRecord> = rdr.records().map(|r| r.unwrap()).collect();
        assert_eq!(records.len(), 4);
        assert_eq!(&records[0][0], "1001");
        assert_eq!(&records[0][1], "Confield");
        assert_eq!(&records[0][2], "UK");
        assert_eq!(&records[3][1], "DOGA");
        assert_eq!(&records[3][2], "AR");

        // Verify release_artist.csv content
        let mut rdr2 = csv::Reader::from_path(dir.path().join("release_artist.csv")).unwrap();
        let records2: Vec<csv::StringRecord> = rdr2.records().map(|r| r.unwrap()).collect();
        assert_eq!(records2.len(), 4);
        assert_eq!(&records2[0][1], "Autechre");
        assert_eq!(&records2[3][1], "Juana Molina");

        // Verify headers via header inspection
        let mut rdr3 = csv::Reader::from_path(dir.path().join("release.csv")).unwrap();
        let headers = rdr3.headers().unwrap();
        assert_eq!(&headers[0], "release_id");
        assert_eq!(&headers[1], "title");
        assert_eq!(&headers[2], "country");
    }

    #[test]
    fn write_via_writer_by_name() {
        let dir = tempfile::tempdir().unwrap();
        let specs = vec![
            CsvFileSpec::new("release.csv", &["id", "title"]),
            CsvFileSpec::new("artist.csv", &["id", "name"]),
            CsvFileSpec::new("label.csv", &["id", "name"]),
        ];
        let mut writer = MultiCsvWriter::new(dir.path(), &specs).unwrap();

        writer.writer_by_name("artist.csv").unwrap()
            .write_record(&["1", "Autechre"]).unwrap();
        writer.writer_by_name("artist.csv").unwrap()
            .write_record(&["2", "Stereolab"]).unwrap();
        writer.writer_by_name("label.csv").unwrap()
            .write_record(&["1", "Warp"]).unwrap();
        writer.writer_by_name("label.csv").unwrap()
            .write_record(&["2", "Duophonic"]).unwrap();
        writer.writer_by_name("release.csv").unwrap()
            .write_record(&["1001", "Confield"]).unwrap();

        writer.flush_all().unwrap();

        // Verify each file independently
        let mut rdr = csv::Reader::from_path(dir.path().join("artist.csv")).unwrap();
        let records: Vec<csv::StringRecord> = rdr.records().map(|r| r.unwrap()).collect();
        assert_eq!(records.len(), 2);
        assert_eq!(&records[0][1], "Autechre");
        assert_eq!(&records[1][1], "Stereolab");

        let mut rdr2 = csv::Reader::from_path(dir.path().join("label.csv")).unwrap();
        let records2: Vec<csv::StringRecord> = rdr2.records().map(|r| r.unwrap()).collect();
        assert_eq!(records2.len(), 2);
        assert_eq!(&records2[0][1], "Warp");
        assert_eq!(&records2[1][1], "Duophonic");
    }

    #[test]
    fn csv_handles_special_characters() {
        // Verify CSV quoting for fields with commas, quotes, newlines
        let dir = tempfile::tempdir().unwrap();
        let specs = vec![CsvFileSpec::new("test.csv", &["artist", "title"])];
        let mut writer = MultiCsvWriter::new(dir.path(), &specs).unwrap();

        writer.writer(0).write_record(&[
            "Duke Ellington & John Coltrane",
            "In a Sentimental Mood",
        ]).unwrap();
        writer.writer(0).write_record(&[
            "Prince Jammy",
            "...Destroys The Space Invaders",
        ]).unwrap();
        writer.writer(0).write_record(&[
            "Father John Misty",
            "I Love You, Honeybear",
        ]).unwrap();
        writer.flush_all().unwrap();

        let mut rdr = csv::Reader::from_path(dir.path().join("test.csv")).unwrap();
        let records: Vec<csv::StringRecord> = rdr.records().map(|r| r.unwrap()).collect();
        assert_eq!(records.len(), 3);
        assert_eq!(&records[0][0], "Duke Ellington & John Coltrane");
        assert_eq!(&records[2][1], "I Love You, Honeybear");
    }

    #[test]
    fn empty_specs_creates_no_files() {
        let dir = tempfile::tempdir().unwrap();
        let specs: Vec<CsvFileSpec> = vec![];
        let mut writer = MultiCsvWriter::new(dir.path(), &specs).unwrap();
        writer.flush_all().unwrap();

        let entries: Vec<_> = std::fs::read_dir(dir.path()).unwrap().collect();
        assert!(entries.is_empty(), "no CSV files should be created for empty specs");
    }

    #[test]
    fn csv_file_spec_construction() {
        let spec = CsvFileSpec::new("release.csv", &["id", "title", "country"]);
        assert_eq!(spec.filename, "release.csv");
        assert_eq!(spec.columns, vec!["id", "title", "country"]);
    }
}
