use std::sync::Arc;

use pyo3::{exceptions::PyRuntimeError, prelude::*};
use strum_macros::{Display, EnumString};
use tokio::runtime::Runtime;

use crate::{
    types::{AssistantSettings, PromptMode, SublimeInputContent},
    worker::OpenAIWorker,
};

#[pyclass(name = "Worker")]
#[derive(Clone, Debug)]
pub struct PythonWorker {
    #[pyo3(get)]
    pub window_id: usize,

    #[pyo3(get)]
    pub proxy: Option<String>,

    worker: OpenAIWorker,
}

struct Function {
    func: Arc<dyn Fn(String) + Send + Sync + 'static>,
}

impl Function {
    fn new(obj: PyObject) -> Self {
        let func = Arc::new(move |s: String| {
            Python::with_gil(|py| {
                let _ = obj.call1(py, (s,));
            });
        });

        Function { func }
    }
}

#[pymethods]
impl PythonWorker {
    #[new]
    #[pyo3(signature = (window_id, path=None, proxy=None))]
    fn new(window_id: usize, path: Option<String>, proxy: Option<String>) -> Self {
        PythonWorker {
            window_id,
            proxy: proxy.clone(),
            worker: OpenAIWorker::new(window_id, path, proxy),
        }
    }

    #[pyo3(signature = (view_id, prompt_mode, contents, assistant_settings, handler))]
    fn run(
        &mut self,
        view_id: usize,
        prompt_mode: PythonPromptMode,
        contents: Vec<SublimeInputContent>,
        assistant_settings: AssistantSettings,
        handler: PyObject,
    ) -> PyResult<()> {
        Runtime::new()?
            .block_on(async {
                self.worker
                    .run(
                        view_id,
                        contents,
                        PromptMode::from(prompt_mode),
                        assistant_settings,
                        Function::new(handler).func,
                    )
                    .await
            })
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }

    pub fn cancel(&mut self) { self.worker.cancel(); }
}

#[pyclass(eq, eq_int)]
#[derive(EnumString, Display, Debug, Clone, Copy, PartialEq)]
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
    use super::*;

    #[test]
    fn test_is_sync_and_send() {
        fn is_sync<T: Sync>() {}
        fn is_send<T: Send>() {}

        is_sync::<PythonWorker>();
        is_send::<PythonWorker>();
        is_send::<PyObject>();
    }
    // This code tested on Python's side
}
