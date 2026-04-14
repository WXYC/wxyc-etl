use pyo3::prelude::*;

#[pymodule]
fn wxyc_etl(_py: Python, _m: &Bound<'_, PyModule>) -> PyResult<()> {
    Ok(())
}
