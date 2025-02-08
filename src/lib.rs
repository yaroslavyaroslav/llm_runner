mod cacher;
mod network_client;
mod openai_network_types;
pub mod types;

mod logger;
mod py_worker;
mod runner;
pub mod stream_handler;
mod tools_definition;
pub mod worker;

use openai_network_types::Roles;
use py_worker::{drop_all, read_all_cache, read_model, write_model, write_to_cache, PythonWorker};
use pyo3::prelude::*;
use types::{
    ApiType,
    AssistantSettings,
    InputKind,
    PromptMode,
    ReasonEffort,
    SublimeInputContent,
    SublimeOutputContent,
};

#[pymodule(name = "llm_runner")]
fn rust_helper(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PythonWorker>()?;
    m.add_class::<AssistantSettings>()?;
    m.add_class::<PromptMode>()?;
    m.add_class::<SublimeInputContent>()?;
    m.add_class::<InputKind>()?;
    m.add_class::<SublimeOutputContent>()?;
    m.add_class::<Roles>()?;
    m.add_class::<ApiType>()?;
    m.add_class::<ReasonEffort>()?;

    m.add_function(wrap_pyfunction!(read_all_cache, m)?)?;
    m.add_function(wrap_pyfunction!(write_to_cache, m)?)?;
    m.add_function(wrap_pyfunction!(drop_all, m)?)?;
    m.add_function(wrap_pyfunction!(read_model, m)?)?;
    m.add_function(wrap_pyfunction!(write_model, m)?)
}
