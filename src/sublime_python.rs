use pyo3::{
    prelude::*,
    sync::GILOnceCell,
    types::{PyDict, PyType},
};

impl FromPyObject<'_> for Settings {
    fn extract_bound(ob: &Bound<'_, PyAny>) -> PyResult<Self> {
        let dict = ob.downcast::<PyDict>()?.clone().unbind();

        Ok(Settings {
            settings_object: dict,
        })
    }
}

#[pyclass]
struct Window {}

// #[derive(IntoPyObject)]
#[pyclass]
pub struct Settings {
    pub settings_object: Py<PyDict>,
}

#[pymethods]
impl Settings {
    // mut settings_object: Item

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
    static SUBLIME: GILOnceCell<Py<PyType>> = GILOnceCell::new();
    let args = (string,);
    // let sublime = PyModule::import(py, "sublime");
    let sublime = SUBLIME.import(py, module, "load_settings")?;
    sublime.call1(args)?.extract().into()
}

#[pymodule]
fn sublime_wrapper(m: &Bound<'_, PyModule>) -> PyResult<()> {
    // m.add_class::<View>()?;
    m.add_class::<Window>()?;
    m.add_class::<Settings>()?;
    m.add_class::<Sheets>()?;
    let _ = m.add_function(wrap_pyfunction!(load_settings, m)?);
    Ok(())
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
