use pyo3::prelude::*;

#[allow(unused)]
pub(crate) fn get_sublime_cache() -> PyResult<String> {
    Python::with_gil(|py| {
        let sublime = PyModule::import(py, "sublime")?;
        let cache_path: String = sublime
            .getattr("cache_path")?
            .call0()?
            .extract()?;
        Ok(cache_path)
    })
}
