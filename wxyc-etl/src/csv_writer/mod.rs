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
}
