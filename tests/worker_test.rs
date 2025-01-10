use reqwest::header::CONTENT_TYPE;
use rust_helper::{types::*, worker::*};
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
#[ignore = "It's paid, so should be skipped by default"]
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
