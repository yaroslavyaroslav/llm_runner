mod cacher;
mod network_client;
mod openai_network_types;
pub mod types;

mod py_worker;
mod sublime_python;
pub mod worker;

use py_worker::{PythonPromptMode, PythonWorker};
use pyo3::prelude::*;
use sublime_python::{load_settings, Settings};
use types::{AssistantSettings, InputKind, OutputMode, SublimeInputContent};

#[pymodule]
fn rust_helper(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Settings>()?;
    let _ = m.add_function(wrap_pyfunction!(load_settings, m)?);

    m.add_class::<PythonPromptMode>()?;
    m.add_class::<PythonWorker>()?;
    m.add_class::<AssistantSettings>()?;
    m.add_class::<OutputMode>()?;
    m.add_class::<SublimeInputContent>()?;
    m.add_class::<InputKind>()?;

    Ok(())
}
