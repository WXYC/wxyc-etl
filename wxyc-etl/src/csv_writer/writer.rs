use std::fs::{self, File};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// Specification for one output CSV file: filename and header columns.
pub struct CsvFileSpec {
    pub filename: String,
    pub columns: Vec<String>,
}

impl CsvFileSpec {
    pub fn new(filename: &str, columns: &[&str]) -> Self {
        Self {
            filename: filename.to_string(),
            columns: columns.iter().map(|s| s.to_string()).collect(),
        }
    }
}

/// Manages N `csv::Writer` instances, one per `CsvFileSpec`, with a shared output directory.
pub struct MultiCsvWriter {
    writers: Vec<csv::Writer<File>>,
    filenames: Vec<String>,
    output_dir: PathBuf,
}

impl MultiCsvWriter {
    /// Create the output directory and open all writers with headers.
    pub fn new(output_dir: &Path, specs: &[CsvFileSpec]) -> Result<Self> {
        fs::create_dir_all(output_dir)
            .with_context(|| format!("creating output directory {}", output_dir.display()))?;

        let mut writers = Vec::with_capacity(specs.len());
        let mut filenames = Vec::with_capacity(specs.len());

        for spec in specs {
            let path = output_dir.join(&spec.filename);
            let mut wtr = csv::Writer::from_path(&path)
                .with_context(|| format!("creating CSV writer for {}", path.display()))?;
            wtr.write_record(&spec.columns)
                .with_context(|| format!("writing headers to {}", spec.filename))?;
            writers.push(wtr);
            filenames.push(spec.filename.clone());
        }

        Ok(Self {
            writers,
            filenames,
            output_dir: output_dir.to_path_buf(),
        })
    }

    /// Get a writer by spec index.
    pub fn writer(&mut self, index: usize) -> &mut csv::Writer<File> {
        &mut self.writers[index]
    }

    /// Get a writer by filename.
    pub fn writer_by_name(&mut self, filename: &str) -> Option<&mut csv::Writer<File>> {
        self.filenames
            .iter()
            .position(|f| f == filename)
            .map(|i| &mut self.writers[i])
    }

    /// Flush all writers.
    pub fn flush_all(&mut self) -> Result<()> {
        for (wtr, name) in self.writers.iter_mut().zip(self.filenames.iter()) {
            wtr.flush()
                .with_context(|| format!("flushing writer for {}", name))?;
        }
        Ok(())
    }

    /// Return the output directory path.
    pub fn output_dir(&self) -> &Path {
        &self.output_dir
    }
}
