use serde_json::from_str;
use std::error::Error;

use crate::cacher::Cacher;
use crate::network_client::NetworkClient;
use crate::openai_network_types::Roles;
use crate::types::{AssistantSettings, CacheEntry, PromptMode, SublimeInputContent};

#[allow(unused, dead_code)]
pub struct OpenAIWorker {
    // TODO: Think on is their necessary to be accessiable through whole object life?
    view_id: Option<usize>,
    window_id: usize,
    prompt_mode: Option<PromptMode>,
    contents: Vec<SublimeInputContent>,
    assistant_settings: Option<AssistantSettings>,
    provider: Option<NetworkClient>,
    cacher: Cacher,
}

impl OpenAIWorker {
    pub fn new(window_id: usize, path: Option<String>) -> Self {
        Self {
            window_id,
            view_id: None,
            prompt_mode: None,
            contents: vec![],
            assistant_settings: None,
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
        self.provider = Some(NetworkClient::new());

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
            self.cacher.write_entry(entry);
        }

        // Preparing payload and request
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

        // Execute network request
        match self
            .provider
            .as_ref()
            .unwrap()
            .execute_response::<serde_json::Value>(request, None)
            .await
        {
            Ok(response) => println!("Response: {:?}", response),
            Err(e) => return Err(format!("Failed to execute network request: {}", e).into()),
        };

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;
    use tempfile::TempDir;
    use tokio::test;
    use wiremock::{
        matchers::{method, path},
        MockServer, ResponseTemplate,
    };

    #[tokio::test]
    async fn test_run_method_with_mock_server() {
        let tmp_dir = TempDir::new()
            .unwrap()
            .into_path()
            .to_str()
            .unwrap()
            .to_string();

        let mut worker = OpenAIWorker::new(1, Some(tmp_dir.clone()));

        // Start a mock server
        let mock_server = MockServer::start().await;

        let endpoint = "/openai/endpoint";

        // Mock the API response
        let _mock = wiremock::Mock::given(method("POST"))
            .and(path(endpoint))
            .respond_with(ResponseTemplate::new(200).set_body_string("{\"response\": \"Success\"}"))
            .mount(&mock_server)
            .await;

        let mut assistant_settings = AssistantSettings::default();
        assistant_settings.url = format!("{}{}", mock_server.uri(), endpoint);
        assistant_settings.token = "dummy-token".to_string();

        let prompt_mode = PromptMode::View;

        let contents = json!([
            {
                "content": "Test content",
                "path": "/path/to/file",
                "scope": "text.plain",
                "input_kind": "viewselection"
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

        let mut worker = OpenAIWorker::new(1, Some(tmp_dir.clone()));

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
}
