use serde_json::from_str;
use std::error::Error;
use tokio::sync::mpsc;

use crate::cacher::Cacher;
use crate::network_client::NetworkClient;
use crate::openai_network_types::{OpenAIResponse, Roles};
use crate::types::{AssistantSettings, CacheEntry, PromptMode, SublimeInputContent};

#[allow(unused, dead_code)]
#[derive(Clone)]
pub struct OpenAIWorker {
    // TODO: Think on is their necessity to be accessiable through whole object life?
    pub(crate) view_id: Option<usize>,
    pub(crate) window_id: usize,
    pub(crate) prompt_mode: Option<PromptMode>,
    pub(crate) contents: Vec<SublimeInputContent>,
    pub(crate) assistant_settings: Option<AssistantSettings>,
    pub(crate) provider: Option<NetworkClient>,
    pub(crate) proxy: Option<String>,
    pub(crate) cacher: Cacher,
}

impl OpenAIWorker {
    pub fn new(window_id: usize, path: Option<String>, proxy: Option<String>) -> Self {
        Self {
            window_id,
            view_id: None,
            prompt_mode: None,
            contents: vec![],
            assistant_settings: None,
            proxy,
            provider: None,
            cacher: Cacher::new(
                // FIXME: This is definitely temporary soludion and should rely on settings in future
                path.unwrap_or("~/Library/Caches/Sublime Text/Cache/OpenAI completion".to_string())
                    .as_str(),
                Some("rust_test"),
            ),
        }
    }

    pub async fn run(
        &mut self,
        view_id: usize,
        contents: String, // encoded `Vec<SublimeInputContent>`
        prompt_mode: PromptMode,
        assistant_settings: AssistantSettings,
    ) -> Result<(), Box<dyn Error>> {
        // Update instance variables
        self.view_id = Some(view_id);
        self.prompt_mode = Some(prompt_mode);
        self.assistant_settings = Some(assistant_settings.clone());
        self.provider = Some(NetworkClient::new(self.proxy.clone()));

        let (tx, rx) = if assistant_settings.stream {
            let (tx, rx) = mpsc::channel(32);
            (Some(tx), Some(rx))
        } else {
            (None, None)
        };

        // Decode the contents
        self.contents = from_str::<Vec<SublimeInputContent>>(&contents)
            .map_err(|e| format!("Failed to decode contents: {}", e))?;

        // Read from cache and extend with new contents
        let mut cache_entries: Vec<CacheEntry> = self.cacher.read_entries()?;
        cache_entries.extend(
            self.contents
                .iter()
                .map(|content| CacheEntry {
                    content: content.content.clone(),
                    path: content.path.clone(),
                    scope: content.scope.clone(),
                    role: Roles::User,
                    tool_call: None,
                    tool_call_id: None,
                })
                .collect::<Vec<_>>(),
        );
        for entry in &self.contents {
            self.cacher.write_entry(&CacheEntry::from(entry.clone()));
        }

        let payload = self
            .provider
            .as_ref()
            .unwrap()
            .prepare_payload(assistant_settings.clone(), vec![], self.contents.clone())
            .map_err(|e| format!("Failed to prepare payload: {}", e))?;

        let request = self
            .provider
            .as_ref()
            .unwrap()
            .prepare_request(assistant_settings.clone(), payload)
            .map_err(|e| format!("Failed to prepare request: {}", e))?;

        let execute_response = if let Some(sender) = tx {
            self.provider
                .as_ref()
                .unwrap()
                .execute_request::<OpenAIResponse>(request, Some(sender))
                .await
        } else {
            self.provider
                .as_ref()
                .unwrap()
                .execute_request::<OpenAIResponse>(request, None)
                .await
        };

        match execute_response {
            Ok(response) => {
                if let Some(mut rx) = rx {
                    while let Some(data) = rx.recv().await {
                        println!("Streaming data: {}", data);
                    }
                }
                println!("Response: {:?}", response);
                Ok(())
            }
            Err(e) => return Err(format!("Failed to execute network request: {}", e).into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::header::CONTENT_TYPE;
    use serde_json::json;
    use std::{env, fs};
    use tempfile::TempDir;
    use tokio::test;
    use wiremock::{
        matchers::{method, path},
        MockServer, ResponseTemplate,
    };

    static PROXY: &str = "127.0.0.1:9090";

    #[tokio::test]
    async fn test_run_method_with_mock_server() {
        let tmp_dir = TempDir::new()
            .unwrap()
            .into_path()
            .to_str()
            .unwrap()
            .to_string();

        let mut worker = OpenAIWorker::new(1, Some(tmp_dir.clone()), Some(PROXY.to_string()));

        // Start a mock server
        let mock_server = MockServer::start().await;

        let endpoint = "/openai/endpoint";

        // Mock the API response
        let _mock = wiremock::Mock::given(method("POST"))
            .and(path(endpoint))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "model": "some_model",
                "id": "some_id",
                "created": 367123,
                "choices": []
            })))
            .mount(&mock_server)
            .await;

        let mut assistant_settings = AssistantSettings::default();
        assistant_settings.url = format!("{}{}", mock_server.uri(), endpoint);
        assistant_settings.token = "dummy-token".to_string();
        assistant_settings.chat_model = "some_model".to_string();
        assistant_settings.stream = false;

        let prompt_mode = PromptMode::View;

        let contents = json!([
            {
                "content": "Test content",
                "path": "/path/to/file",
                "scope": "text.plain",
                "input_kind": "view_selection"
            }
        ])
        .to_string();

        let result = worker
            .run(1, contents, prompt_mode, assistant_settings)
            .await;

        assert!(result.is_ok(), "Expected Ok, got Err: {:?}", result);
        assert!(fs::remove_dir_all(tmp_dir).is_ok())
    }

    #[tokio::test]
    async fn test_run_method_see_with_mock_server() {
        let tmp_dir = TempDir::new()
            .unwrap()
            .into_path()
            .to_str()
            .unwrap()
            .to_string();

        let mut worker = OpenAIWorker::new(1, Some(tmp_dir.clone()), Some(PROXY.to_string()));

        // Start a mock server
        let mock_server = MockServer::start().await;

        let endpoint = "/openai/endpoint";

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

        let mut assistant_settings = AssistantSettings::default();
        assistant_settings.url = format!("{}{}", mock_server.uri(), endpoint);
        assistant_settings.token = "dummy-token".to_string();
        assistant_settings.chat_model = "some_model".to_string();
        assistant_settings.stream = true;

        let prompt_mode = PromptMode::View;

        let contents = json!([
            {
                "content": "Test content",
                "path": "/path/to/file",
                "scope": "text.plain",
                "input_kind": "view_selection"
            }
        ])
        .to_string();

        let result = worker
            .run(1, contents, prompt_mode, assistant_settings)
            .await;

        assert!(result.is_ok(), "Expected Ok, got Err: {:?}", result);
        assert!(fs::remove_dir_all(tmp_dir).is_ok())
    }

    #[test]
    async fn test_run_method_failure() {
        let tmp_dir = TempDir::new()
            .unwrap()
            .into_path()
            .to_str()
            .unwrap()
            .to_string();

        let mut worker = OpenAIWorker::new(1, Some(tmp_dir.clone()), Some(PROXY.to_string()));

        let assistant_settings = AssistantSettings::default();
        let prompt_mode = PromptMode::View;
        let invalid_contents = "not valid json";

        let result = worker
            .run(
                1,
                invalid_contents.to_string(),
                prompt_mode,
                assistant_settings,
            )
            .await;

        assert!(
            result.is_err(),
            "Expected Err, got Ok with value: {:?}",
            result
        );
        assert!(fs::remove_dir_all(tmp_dir).is_ok())
    }

    // HINT: Disabled becuase it's paid
    #[test]
    async fn test_remote_server() {
        let tmp_dir = TempDir::new()
            .unwrap()
            .into_path()
            .to_str()
            .unwrap()
            .to_string();

        let mut worker = OpenAIWorker::new(1, Some(tmp_dir.clone()), Some(PROXY.to_string()));

        let mut assistant_settings = AssistantSettings::default();
        assistant_settings.url = format!("https://api.openai.com/v1/chat/completions");
        assistant_settings.token = env::var("OPENAI_API_TOKEN").unwrap();
        assistant_settings.chat_model = "gpt-4o-mini".to_string();
        assistant_settings.stream = true;

        let prompt_mode = PromptMode::View;

        let contents = json!([
            {
                "content": "This is the test request, provide me 300 words response",
                "path": "/path/to/file",
                "scope": "text.plain",
                "input_kind": "view_selection"
            }
        ])
        .to_string();

        let result = worker
            .run(1, contents, prompt_mode, assistant_settings)
            .await;

        assert!(result.is_ok(), "Expected Ok, got Err: {:?}", result);
    }
}
