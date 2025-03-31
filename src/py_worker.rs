use std::{
    sync::{Arc, atomic::Ordering},
    thread,
};

use pyo3::prelude::*;
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

    worker: Arc<OpenAIWorker>,
}

struct TextHandler {
    func: Arc<dyn Fn(String) + Send + Sync + 'static>,
}

impl TextHandler {
    fn new(obj: PyObject) -> Self {
        let func = Arc::new(move |s: String| {
            Python::with_gil(|py| {
                let _ = obj.call1(py, (s,));
            });
        });

        TextHandler { func }
    }
}

struct FunctionHandler {
    func: Arc<dyn Fn((String, String)) -> String + Send + Sync + 'static>,
}

impl FunctionHandler {
    fn new(obj: PyObject) -> Self {
        let func = Arc::new(
            move |args: (String, String)| -> String {
                Python::with_gil(|py| {
                    obj.call1(py, args)
                        .and_then(|ret| ret.extract::<String>(py))
                        .expect("Python function call or extraction failed")
                })
            },
        );
        Self { func }
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
            worker: Arc::new(OpenAIWorker::new(
                window_id, path, proxy,
            )),
        }
    }

    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (view_id, prompt_mode, contents, assistant_settings, handler, error_handler, function_handler))]
    fn run(
        &mut self,
        view_id: usize,
        prompt_mode: PromptMode,
        contents: Vec<SublimeInputContent>,
        assistant_settings: AssistantSettings,
        handler: PyObject,
        error_handler: PyObject,
        function_handler: PyObject,
    ) -> PyResult<()> {
        let rt = Runtime::new().expect("Failed to create runtime");
        let worker_clone = self.worker.clone();
        thread::spawn(move || {
            rt.block_on(async move {
                worker_clone
                    .run(
                        view_id,
                        contents,
                        prompt_mode,
                        assistant_settings,
                        TextHandler::new(handler).func,
                        TextHandler::new(error_handler).func,
                        FunctionHandler::new(function_handler).func,
                    )
                    .await
            })
        });

        Ok(())
    }

    pub fn cancel(&mut self) { self.worker.cancel() }

    pub fn is_alive(&self) -> bool {
        self.worker
            .is_alive
            .load(Ordering::Relaxed)
    }

    #[allow(clippy::too_many_arguments)]
    fn run_sync(
        &mut self,
        view_id: usize,
        prompt_mode: PromptMode,
        contents: Vec<SublimeInputContent>,
        assistant_settings: AssistantSettings,
        handler: PyObject,
        error_handler: PyObject,
        function_handler: PyObject,
    ) -> PyResult<()> {
        let rt = Runtime::new().expect("Failed to create runtime");
        let worker_clone = self.worker.clone();
        let _ = rt.block_on(async move {
            worker_clone
                .run(
                    view_id,
                    contents,
                    prompt_mode,
                    assistant_settings,
                    TextHandler::new(handler).func,
                    TextHandler::new(error_handler).func,
                    FunctionHandler::new(function_handler).func,
                )
                .await
        });

        Ok(())
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
pub fn read_model(path: &str) -> PyResult<AssistantSettings> {
    let cacher = Cacher::new(path);
    let model = cacher
        .read_model::<AssistantSettings>()
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("{}", e)))?;

    Ok(model)
}

#[pyfunction]
#[allow(unused)]
#[pyo3(signature = (path, model))]
pub fn write_model(path: &str, model: AssistantSettings) -> PyResult<()> {
    println!("path in rust: {}", path);
    let cacher = Cacher::new(path);
    cacher.write_model::<AssistantSettings>(&model);
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
