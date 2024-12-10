use pyo3::{prelude::*, types::PyDict};

impl FromPyObject<'_> for Settings {
    fn extract_bound(ob: &Bound<'_, PyAny>) -> PyResult<Self> {
        let dict = ob.downcast::<PyDict>()?.clone().unbind();

        Ok(Settings {
            settings_object: dict,
        })
    }
}

#[pyclass]
pub struct Window {}

#[pyclass]
pub struct Settings {
    pub settings_object: Py<PyDict>,
}

#[pymethods]
impl Settings {
    pub fn get(&self, py: Python, key: &str) -> PyResult<PyObject> {
        let dict = self.settings_object.clone_ref(py);
        let another_dict = dict.into_bound(py);
        let value = another_dict.get_item(key)?.unwrap();

        Ok(value.unbind())
    }
}

#[pyclass]
pub struct Sheets {}

#[pyfunction(text_signature = "(module='default_module')")]
pub fn load_settings<'py>(py: Python<'py>, module: &str, string: &str) -> PyResult<Settings> {
    let function_name = "load_settings";
    let func = py.import(module)?.getattr(function_name)?;
    let args = (string,);
    let settings = func.call1(args)?;

    settings.extract().into()
}

// Mock sublime module for testing
pub fn create_mock_sublime_module(py: Python) -> PyResult<Py<PyModule>> {
    let module = PyModule::new(py, "sublime")?;
    let settings_dict = PyDict::new(py);

    // Add mock data to the settings dictionary
    settings_dict.set_item("some_key", "some_value")?;

    module.setattr("settings", settings_dict)?;
    Ok(module.into())
}
