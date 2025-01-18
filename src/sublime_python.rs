use pyo3::{prelude::*, types::PyDict};

impl FromPyObject<'_> for Settings {
    fn extract_bound(ob: &Bound<'_, PyAny>) -> PyResult<Self> {
        let dict = ob
            .downcast::<PyDict>()?
            .clone()
            .unbind();

        Ok(Settings {
            settings_object: dict,
        })
    }
}

#[pyclass]
pub struct Settings {
    pub settings_object: Py<PyDict>,
}

pub fn get_sublime_cache() -> PyResult<String> {
    Python::with_gil(|py| {
        let sublime = PyModule::import(py, "sublime")?;
        let cache_path: String = sublime
            .getattr("cache_path")?
            .call0()?
            .extract()?;
        Ok(cache_path)
    })
}

#[pymethods]
impl Settings {
    pub fn get(&self, py: Python, key: &str) -> PyResult<PyObject> {
        let dict = self
            .settings_object
            .clone_ref(py)
            .into_bound(py);
        let value = dict.get_item(key)?.unwrap();

        Ok(value.unbind())
    }

    pub fn set(&self, py: Python, key: &str, value: PyObject) -> PyResult<()> {
        let dict = self
            .settings_object
            .clone_ref(py)
            .into_bound(py);
        dict.set_item(key, value)
    }
}

#[pyfunction(text_signature = "(module='default_module')")]
pub fn load_settings(py: Python, module: &str, string: &str) -> PyResult<Settings> {
    let function_name = "load_settings";
    let func = py
        .import(module)?
        .getattr(function_name)?;
    let args = (string,);
    let settings = func.call1(args)?;

    settings.extract()
}
