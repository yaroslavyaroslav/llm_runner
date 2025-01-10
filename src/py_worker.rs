// use std::collections::HashMap;

use pyo3::{exceptions::PyRuntimeError, prelude::*};
use strum_macros::{Display, EnumString};
use tokio::runtime::Runtime;

use crate::{
    types::{AssistantSettings, PromptMode, SublimeInputContent},
    worker::OpenAIWorker,
};

#[pyclass]
#[derive(FromPyObject)]
pub struct PythonWorker {
    #[pyo3(get)]
    pub window_id: usize,

    #[pyo3(get)]
    pub view_id: Option<usize>,

    #[pyo3(get)]
    pub prompt_mode: Option<PythonPromptMode>,

    #[pyo3(get)]
    pub contents: Option<String>,

    #[pyo3(get)]
    pub proxy: Option<String>,

    cacher_path: String,
}

#[pymethods]
impl PythonWorker {
    #[new]
    #[pyo3(signature = (window_id, path, proxy=None))]
    fn new(window_id: usize, path: String, proxy: Option<String>) -> Self {
        PythonWorker { window_id, view_id: None, prompt_mode: None, contents: None, cacher_path: path, proxy }
    }

    #[pyo3(signature = (view_id, prompt_mode, contents, assistant_settings))]
    fn run(
        &self,
        view_id: usize,
        prompt_mode: PythonPromptMode,
        contents: Vec<SublimeInputContent>,
        assistant_settings: AssistantSettings,
    ) -> PyResult<()> {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            #[rustfmt::skip]
            let mut worker = OpenAIWorker::new(self.window_id, self.cacher_path.clone(), self.proxy.clone());

            worker
                .run(view_id, contents, PromptMode::from(prompt_mode), assistant_settings)
                .await
        })
        .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }
}

#[pyclass(eq, eq_int)]
#[derive(EnumString, Display, Clone, Copy, PartialEq)]
pub enum PythonPromptMode {
    #[strum(serialize = "view")]
    View,
    #[strum(serialize = "phantom")]
    Phantom,
}

impl From<PromptMode> for PythonPromptMode {
    fn from(mode: PromptMode) -> Self {
        match mode {
            PromptMode::View => PythonPromptMode::View,
            PromptMode::Phantom => PythonPromptMode::Phantom,
        }
    }
}

impl From<PythonPromptMode> for PromptMode {
    fn from(py_mode: PythonPromptMode) -> Self {
        match py_mode {
            PythonPromptMode::View => PromptMode::View,
            PythonPromptMode::Phantom => PromptMode::Phantom,
        }
    }
}

#[pymethods]
impl PythonPromptMode {
    #[staticmethod]
    pub fn from_str(s: &str) -> Option<PythonPromptMode> {
        match s.to_lowercase().as_str() {
            "view" => Some(PythonPromptMode::View),
            "phantom" => Some(PythonPromptMode::Phantom),
            _ => None, // Handle invalid input by returning None
        }
    }

    #[staticmethod]
    pub fn to_str(py_mode: PythonPromptMode) -> String {
        match py_mode {
            PythonPromptMode::View => "view".to_string(),
            PythonPromptMode::Phantom => "phantom".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    // This code tested on Python's side
}
