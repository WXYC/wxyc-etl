use pyo3::prelude::*;

mod text;
mod parser;
mod state;
mod import_utils;
mod schema;

#[pymodule]
fn wxyc_etl(py: Python, m: &Bound<'_, PyModule>) -> PyResult<()> {
    let text_mod = PyModule::new(py, "text")?;
    text::register(&text_mod)?;
    m.add_submodule(&text_mod)?;

    let parser_mod = PyModule::new(py, "parser")?;
    parser::register(&parser_mod)?;
    m.add_submodule(&parser_mod)?;

    let state_mod = PyModule::new(py, "state")?;
    state::register(&state_mod)?;
    m.add_submodule(&state_mod)?;

    let import_utils_mod = PyModule::new(py, "import_utils")?;
    import_utils::register(&import_utils_mod)?;
    m.add_submodule(&import_utils_mod)?;

    let schema_mod = PyModule::new(py, "schema")?;
    schema::register(&schema_mod)?;
    m.add_submodule(&schema_mod)?;

    Ok(())
}
