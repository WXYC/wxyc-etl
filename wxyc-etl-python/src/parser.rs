use std::path::Path;

use pyo3::prelude::*;
use pyo3::types::PyList;

use wxyc_etl::parser::mysql::{self, SqlValue};

/// Register parser submodule functions.
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(load_table_rows, m)?)?;
    m.add_function(wrap_pyfunction!(iter_table_rows, m)?)?;
    m.add_function(wrap_pyfunction!(parse_sql_values, m)?)?;
    Ok(())
}

/// Convert a SqlValue to a Python object.
fn sql_value_to_py(py: Python, val: &SqlValue) -> PyObject {
    match val {
        SqlValue::Null => py.None(),
        SqlValue::Int(n) => n.into_pyobject(py).unwrap().into_any().unbind(),
        SqlValue::Float(f) => f.into_pyobject(py).unwrap().into_any().unbind(),
        SqlValue::Str(s) => s.into_pyobject(py).unwrap().into_any().unbind(),
    }
}

/// Convert a row of SqlValues to a Python tuple.
fn row_to_py_tuple(py: Python, row: &[SqlValue]) -> PyObject {
    let elements: Vec<PyObject> = row.iter().map(|v| sql_value_to_py(py, v)).collect();
    pyo3::types::PyTuple::new(py, &elements)
        .unwrap()
        .into_any()
        .unbind()
}

/// Load all rows for a given table from a MySQL dump file.
///
/// Returns a list of tuples, matching the sql_parser_rs API.
#[pyfunction]
fn load_table_rows(py: Python, path: &str, table_name: &str) -> PyResult<Py<PyList>> {
    let rows = mysql::load_table_rows(Path::new(path), table_name)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

    let py_rows: Vec<PyObject> = rows.iter().map(|r| row_to_py_tuple(py, r)).collect();
    let list = PyList::new(py, &py_rows)?;
    Ok(list.unbind())
}

/// Load all rows for a given table (same as load_table_rows for API compatibility).
///
/// In Rust, iter_table_rows uses a callback. For Python compatibility, this
/// collects all rows and returns them as a list, matching the sql_parser_rs API.
#[pyfunction]
fn iter_table_rows(py: Python, path: &str, table_name: &str) -> PyResult<Py<PyList>> {
    load_table_rows(py, path, table_name)
}

/// Parse a single INSERT VALUES clause.
///
/// Takes a string like "(1,'a'),(2,'b')" and returns a list of lists.
#[pyfunction]
fn parse_sql_values(py: Python, line: &str) -> PyResult<Vec<Vec<PyObject>>> {
    let rows = mysql::parse_sql_values(line.as_bytes());
    let result: Vec<Vec<PyObject>> = rows
        .iter()
        .map(|row| row.iter().map(|v| sql_value_to_py(py, v)).collect())
        .collect();
    Ok(result)
}
