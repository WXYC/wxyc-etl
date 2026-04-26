use std::path::Path;

use pyo3::prelude::*;

use wxyc_etl::state::PipelineState as RustPipelineState;

/// Register state submodule.
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PipelineState>()?;
    Ok(())
}

/// Convert an anyhow error to a PyErr.
fn to_py_runtime_err(e: anyhow::Error) -> PyErr {
    pyo3::exceptions::PyRuntimeError::new_err(e.to_string())
}

fn to_py_value_err(e: anyhow::Error) -> PyErr {
    pyo3::exceptions::PyValueError::new_err(e.to_string())
}

/// Tracks step completion status for resumable ETL runs.
#[pyclass]
#[derive(Debug)]
struct PipelineState {
    inner: RustPipelineState,
}

#[pymethods]
impl PipelineState {
    #[new]
    fn new(db_url: &str, csv_dir: &str, steps: Vec<String>) -> Self {
        let step_refs: Vec<&str> = steps.iter().map(|s| s.as_str()).collect();
        Self {
            inner: RustPipelineState::new(db_url, csv_dir, &step_refs),
        }
    }

    /// Return true if the step has been completed.
    fn is_completed(&self, step: &str) -> bool {
        self.inner.is_completed(step)
    }

    /// Mark a step as completed.
    fn mark_completed(&mut self, step: &str) {
        self.inner.mark_completed(step);
    }

    /// Mark a step as failed with an error message.
    fn mark_failed(&mut self, step: &str, error: &str) {
        self.inner.mark_failed(step, error);
    }

    /// Return the status string of a step ("pending", "completed", "failed", "unknown").
    fn step_status(&self, step: &str) -> String {
        self.inner.step_status(step).to_string()
    }

    /// Return the error message for a failed step, or None.
    fn step_error(&self, step: &str) -> Option<String> {
        self.inner.step_error(step).map(String::from)
    }

    /// Raise an error if db_url or csv_dir don't match this state.
    fn validate_resume(&self, db_url: &str, csv_dir: &str) -> PyResult<()> {
        self.inner
            .validate_resume(db_url, csv_dir)
            .map_err(to_py_value_err)
    }

    /// Write state to a JSON file.
    fn save(&self, path: &str) -> PyResult<()> {
        self.inner.save(Path::new(path)).map_err(to_py_runtime_err)
    }

    /// Load state from a JSON file with version migration support.
    #[staticmethod]
    fn load(path: &str) -> PyResult<Self> {
        let inner = RustPipelineState::load(Path::new(path)).map_err(to_py_runtime_err)?;
        Ok(Self { inner })
    }
}
