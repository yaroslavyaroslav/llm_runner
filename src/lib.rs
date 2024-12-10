pub mod cacher;
pub mod network_client;
pub mod sublime_python;
pub mod types;
pub mod worker;

use pyo3::prelude::*;
use sublime_python::{load_settings, Settings, Sheets, Window};

#[pymodule]
fn rust_helper(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Settings>()?;
    m.add_class::<Sheets>()?;
    m.add_class::<Window>()?;
    let _ = m.add_function(wrap_pyfunction!(load_settings, m)?);
    Ok(())
}
