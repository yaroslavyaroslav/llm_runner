use std::{
    sync::{Arc, Mutex},
    thread,
};

use pyo3::prelude::*;
use strum_macros::{Display, EnumString};
use tokio::runtime::Runtime;

use crate::{
    cacher::Cacher,
    types::{AssistantSettings, CacheEntry, PromptMode, SublimeInputContent, SublimeOutputContent},
    worker::OpenAIWorker,
};

#[pyclass(name = "Worker")]
#[derive(Clone, Debug)]
pub struct PythonWorker {
    #[pyo3(get)]
    pub window_id: usize,

    #[pyo3(get)]
    pub proxy: Option<String>,

    worker: Arc<Mutex<OpenAIWorker>>,
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
    #[pyo3(signature = (window_id, path, proxy=None))]
    fn new(window_id: usize, path: String, proxy: Option<String>) -> Self {
        PythonWorker {
            window_id,
            proxy: proxy.clone(),
            worker: Arc::new(Mutex::new(OpenAIWorker::new(
                window_id, path, proxy,
            ))),
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
        let rt = Runtime::new().expect("Failed to create runtime");
        let worker_clone = self.worker.clone();
        thread::spawn(move || {
            let result = rt.block_on(async move {
                worker_clone
                    .lock()
                    .unwrap()
                    .run(
                        view_id,
                        contents,
                        PromptMode::from(prompt_mode),
                        assistant_settings,
                        Function::new(handler).func,
                    )
                    .await
            });

            result
        });

        Ok(())
    }

    pub fn cancel(&mut self) {
        self.worker
            .lock()
            .unwrap()
            .cancel();
    }
}

#[pyfunction]
#[allow(unused)]
#[pyo3(signature = (path))]
pub fn read_all_cache(path: &str) -> PyResult<Vec<SublimeOutputContent>> {
    let cacher = Cacher::new(path);
    let cache_entries = cacher
        .read_entries::<CacheEntry>()
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("{}", e)))?;

    let vec = cache_entries
        .iter()
        .map(SublimeOutputContent::from)
        .collect();

    Ok(vec)
}

#[pyfunction]
#[allow(unused)]
#[pyo3(signature = (path, content))]
pub fn write_to_cache(path: &str, content: SublimeInputContent) -> PyResult<()> {
    let entry = CacheEntry::from(content);

    let cacher = Cacher::new(path);
    cacher.write_entry::<CacheEntry>(&entry);
    Ok(())
}

#[pyfunction]
#[allow(unused)]
#[pyo3(signature = (path))]
pub fn drop_all(path: &str) -> PyResult<()> {
    let cacher = Cacher::new(path);
    cacher.drop_all();
    Ok(())
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
