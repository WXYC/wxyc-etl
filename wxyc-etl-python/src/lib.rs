use pyo3::prelude::*;
use pyo3::types::PyModuleMethods;

mod text;
mod parser;
mod state;
mod import_utils;
mod schema;

/// Register a submodule and add it to sys.modules so `from wxyc_etl.X import Y` works.
fn register_submodule(
    py: Python,
    parent: &Bound<'_, PyModule>,
    name: &str,
    register_fn: fn(&Bound<'_, PyModule>) -> PyResult<()>,
) -> PyResult<()> {
    let sub = PyModule::new(py, name)?;
    register_fn(&sub)?;
    parent.add_submodule(&sub)?;

    // Register in sys.modules so `from wxyc_etl.name import ...` works
    let full_name = format!("wxyc_etl.{name}");
    py.import("sys")?
        .getattr("modules")?
        .set_item(&full_name, sub)?;

    Ok(())
}

#[pymodule]
fn wxyc_etl(py: Python, m: &Bound<'_, PyModule>) -> PyResult<()> {
    register_submodule(py, m, "text", text::register)?;
    register_submodule(py, m, "parser", parser::register)?;
    register_submodule(py, m, "state", state::register)?;
    register_submodule(py, m, "import_utils", import_utils::register)?;
    register_submodule(py, m, "schema", schema::register)?;
    Ok(())
}
