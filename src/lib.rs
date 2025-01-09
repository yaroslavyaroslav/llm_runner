mod cacher;
mod network_client;
mod openai_network_types;
mod types;

mod py_worker;
pub mod sublime_python;
pub mod worker;

use py_worker::{PythonPromptMode, PythonWorker};
use pyo3::prelude::*;
use sublime_python::{load_settings, Settings};

#[pymodule]
fn rust_helper(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Settings>()?;
    let _ = m.add_function(wrap_pyfunction!(load_settings, m)?);

    m.add_class::<PythonPromptMode>()?;
    m.add_class::<PythonWorker>()?;

    Ok(())
}
