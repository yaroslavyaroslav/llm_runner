mod cacher;
mod network_client;
mod openai_network_types;
mod types;

pub mod sublime_python;
pub mod worker;

use pyo3::prelude::*;
use sublime_python::{load_settings, Settings};

#[pymodule]
fn rust_helper(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Settings>()?;
    let _ = m.add_function(wrap_pyfunction!(load_settings, m)?);
    Ok(())
}
