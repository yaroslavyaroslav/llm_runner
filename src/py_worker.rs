// use std::collections::HashMap;

use pyo3::{exceptions::PyRuntimeError, prelude::*};
use strum_macros::{Display, EnumString};
use tokio::runtime::Runtime;

use crate::{
    types::{AssistantSettings, PromptMode},
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
}

#[pymethods]
impl PythonWorker {
    #[new]
    #[pyo3(signature = (window_id, proxy=None))]
    fn new(window_id: usize, proxy: Option<String>) -> Self {
        PythonWorker {
            window_id,
            view_id: None,
            prompt_mode: None,
            contents: None,
            proxy,
        }
    }

    #[pyo3(signature = (view_id, prompt_mode, contents, assistant_settings))]
    fn run(
        &self,
        view_id: usize,
        prompt_mode: PythonPromptMode,
        contents: String,
        assistant_settings: String,
    ) -> PyResult<()> {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let mut worker = OpenAIWorker::new(
                self.window_id,
                Some("/tmp/".to_string()),
                self.proxy.clone(),
            );
            worker
                .run(
                    view_id,
                    contents,
                    PromptMode::from(prompt_mode),
                    serde_json::from_str::<AssistantSettings>(assistant_settings.as_str()).unwrap(),
                )
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
