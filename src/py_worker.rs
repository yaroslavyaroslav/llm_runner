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
    fn new(window_id: usize, proxy: Option<String>) -> Self {
        PythonWorker {
            window_id,
            view_id: None,
            prompt_mode: None,
            contents: None,
            proxy,
        }
    }

    pub fn run(
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
    use super::*;
    use crate::types::{AssistantSettings, InputKind};
    use reqwest::header::CONTENT_TYPE;
    use serde_json::json;
    use tempfile::TempDir;
    use wiremock::matchers::{method, path};
    use wiremock::{MockServer, ResponseTemplate};

    static PROXY: &str = "127.0.0.1:9090";

    #[tokio::test]
    async fn test_python_worker_run() {
        // Create a temporary directory for caching
        let tmp_dir = TempDir::new().unwrap();
        let tmp_path = tmp_dir.path().to_str().unwrap().to_string();

        // Create an instance of the worker
        let mut worker = OpenAIWorker::new(1, Some(tmp_path.clone()), Some(PROXY.to_string()));

        // Start a mock server
        let mock_server = MockServer::start().await;
        let endpoint = "/v1/chat/completions";

        // Mock the API response
        let sse_data = r#"
        data: {"id":"8f18fa2f381e5b8e-VIE","object":"chat.completion.chunk","created":1734124608,"model":"model", "choices":[{"delta":{"content":"The","role":"assistant","tool_calls":null},"finish_reason":null,"index":0}],"created":1734374933,"id":"cmpl-9775b1b7-0746-470e-a541-e0cc8f73bcce","model":"Llama-3.3-70B-Instruct","object":"chat.completion.chunk","usage":null}

        data: {"id":"8f18fa2f381e5b8e-VIE","object":"chat.completion.chunk","created":1734124608,"model":"model", "choices":[{"delta":{"content":" ","role":"assistant","tool_calls":null},"finish_reason":null,"index":0}],"created":1734374933,"id":"cmpl-9775b1b7-0746-470e-a541-e0cc8f73bcce","model":"Llama-3.3-70B-Instruct","object":"chat.completion.chunk","usage":null}

        data: {"id":"8f18fa2f381e5b8e-VIE","object":"chat.completion.chunk","created":1734124608,"model":"model", "choices":[{"delta":{"content":"202","role":"assistant","tool_calls":null},"finish_reason":null,"index":0}],"created":1734374933,"id":"cmpl-9775b1b7-0746-470e-a541-e0cc8f73bcce","model":"Llama-3.3-70B-Instruct","object":"chat.completion.chunk","usage":null}

        data: [DONE]
        "#;

        // Mock the API response
        let _mock = wiremock::Mock::given(method("POST"))
            .and(path(endpoint))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header(CONTENT_TYPE.as_str(), "application/json")
                    .set_body_string(sse_data),
            )
            .mount(&mock_server)
            .await;

        // Set up the assistant settings
        let mut assistant_settings = AssistantSettings::default();
        assistant_settings.url = format!("{}{}", mock_server.uri(), endpoint);
        assistant_settings.token = "dummy-token".to_string();
        assistant_settings.chat_model = "gpt-4o-mini".to_string();
        assistant_settings.stream = true;

        // Input test contents
        let contents = json!([
            {
                "content": "Test content",
                "path": "/path/to/file",
                "scope": "text.plain",
                "input_kind": InputKind::ViewSelection,
            }
        ])
        .to_string();

        // Run the worker
        let result = worker
            .run(1, contents, PromptMode::View, assistant_settings)
            .await;

        // Assert that the result is OK
        assert!(result.is_ok(), "Expected Ok, got: {:?}", result);
        // Optionally we can check the internal state or ensure the cache has been updated.
    }
}
