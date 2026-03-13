mod common;

use std::{
    env,
    fs,
    sync::{Arc, Mutex},
};

use common::mocks::{RecordedSequentialResponder, SequentialResponder, SseEvent, sse_response};
use llm_runner::{types::*, worker::*};
// use reqwest::header::CONTENT_TYPE;
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::test;
use wiremock::{
    Mock,
    MockServer,
    ResponseTemplate,
    matchers::{method, path},
};

#[tokio::test]
async fn test_run_chact_method_with_mock_server() {
    let tmp_dir = TempDir::new()
        .unwrap()
        .into_path()
        .to_str()
        .unwrap()
        .to_string();

    let worker = OpenAIWorker::new(1, tmp_dir.clone(), None);

    // Start a mock server
    let mock_server = MockServer::start().await;

    let endpoint = "/openai/endpoint";

    // Mock the API response
    let _mock = wiremock::Mock::given(method("POST"))
        .and(path(endpoint))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({
                "model": "some_model",
                "id": "some_id",
                "created": 367123,
                "choices": [{
                    "index": 0,
                    "message": {
                        "role": "assistant",
                        "content": "Some Content",
                        "refusal": null
                    },
                    "logprobs": null,
                    "finish_reason": "stop"
                }]
            })),
        )
        .mount(&mock_server)
        .await;

    let mut assistant_settings = AssistantSettings::default();
    assistant_settings.url = format!("{}{}", mock_server.uri(), endpoint);
    assistant_settings.token = Some("dummy-token".to_string());
    assistant_settings.chat_model = "some_model".to_string();
    assistant_settings.stream = false;

    let prompt_mode = PromptMode::View;

    let contents = SublimeInputContent {
        content: Some("This is the test request, provide me 300 words response".to_string()),
        path: Some("/path/to/file".to_string()),
        scope: Some("text.plain".to_string()),
        input_kind: InputKind::ViewSelection,
        tool_id: None,
    };

    let result = worker
        .run(
            1,
            vec![contents],
            prompt_mode,
            assistant_settings,
            Arc::new(|_| {}),
            Arc::new(|_| {}),
            Arc::new(|_| "".to_string()),
        )
        .await;

    assert!(
        result.is_ok(),
        "Expected Ok, got Err: {:?}",
        result
    );
    assert!(fs::remove_dir_all(tmp_dir).is_ok())
}

#[tokio::test]
async fn test_run_tool_method_with_mock_server() {
    let tmp_dir = TempDir::new()
        .unwrap()
        .into_path()
        .to_str()
        .unwrap()
        .to_string();

    let worker = OpenAIWorker::new(1, tmp_dir.clone(), None);

    // Start a mock server
    let mock_server = MockServer::start().await;

    let endpoint = "/openai/endpoint";

    let responder = SequentialResponder::new();

    wiremock::Mock::given(method("POST"))
        .and(path(endpoint))
        .respond_with(responder)
        .mount(&mock_server)
        .await;

    let mut assistant_settings = AssistantSettings::default();
    assistant_settings.url = format!("{}{}", mock_server.uri(), endpoint);
    assistant_settings.token = Some("dummy-token".to_string());
    assistant_settings.chat_model = "some_model".to_string();
    assistant_settings.stream = false;
    assistant_settings.api_type = ApiType::OpenAi;

    let prompt_mode = PromptMode::View;

    let contents = SublimeInputContent {
        content: Some("This is the test request, provide me 300 words response".to_string()),
        path: Some("/path/to/file".to_string()),
        scope: Some("text.plain".to_string()),
        input_kind: InputKind::ViewSelection,
        tool_id: None,
    };

    let result = worker
        .run(
            1,
            vec![contents],
            prompt_mode,
            assistant_settings,
            Arc::new(|_| {}),
            Arc::new(|_| {}),
            Arc::new(|_| "".to_string()),
        )
        .await;

    assert!(
        result.is_ok(),
        "Expected Ok, got Err: {:?}",
        result
    );
    assert!(fs::remove_dir_all(tmp_dir).is_ok())
}

#[tokio::test]
async fn test_error_handler_called_on_http_failure() {
    // Setup temporary cache folder.
    let tmp_dir = TempDir::new()
        .unwrap()
        .into_path()
        .to_str()
        .unwrap()
        .to_string();
    let worker = OpenAIWorker::new(1, tmp_dir.clone(), None);

    // Start a mock server that returns a 500 error.
    let mock_server = MockServer::start().await;
    let endpoint = "/openai/endpoint";
    let _mock = Mock::given(method("POST"))
        .and(path(endpoint))
        .respond_with(ResponseTemplate::new(500))
        .mount(&mock_server)
        .await;

    let mut assistant_settings = AssistantSettings::default();
    assistant_settings.url = format!("{}{}", mock_server.uri(), endpoint);
    assistant_settings.token = Some("dummy-token".to_string());
    assistant_settings.chat_model = "some_model".to_string();
    assistant_settings.stream = false;

    // Create an error accumulator.
    let errors: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(vec![]));
    let errors_clone = errors.clone();
    let error_handler = Arc::new(move |msg: String| {
        let mut guard = errors_clone.lock().unwrap();
        guard.push(msg);
    });

    // A normal handler that does nothing.
    let normal_handler = Arc::new(|_s: String| {});

    let contents = vec![SublimeInputContent {
        content: Some("trigger error".to_string()),
        path: Some("dummy".to_string()),
        scope: Some("dummy".to_string()),
        input_kind: InputKind::ViewSelection,
        tool_id: None,
    }];

    let result = worker
        .run(
            1,
            contents,
            PromptMode::View,
            assistant_settings,
            normal_handler,
            error_handler,
            Arc::new(|_| "".to_string()),
        )
        .await;

    // Expect an error result due to the 500 response.
    assert!(
        result.is_err(),
        "Expected error result due to failing endpoint"
    );

    let errs = dbg!(errors)
        .lock()
        .unwrap()
        .clone();
    assert!(
        !errs.is_empty(),
        "Expected error_handler to be called when LlmRunner fails"
    );
    assert!(
        errs.iter()
            .any(|msg| msg.contains("LlmRunner error")),
        "Expected error message to contain 'LlmRunner error'"
    );

    let _ = fs::remove_dir_all(tmp_dir);
}

#[tokio::test]
async fn test_error_handler_not_called_on_success() {
    // Setup temporary cache folder.
    let tmp_dir = TempDir::new()
        .unwrap()
        .into_path()
        .to_str()
        .unwrap()
        .to_string();
    let worker = OpenAIWorker::new(1, tmp_dir.clone(), None);

    // Start a mock server that returns a successful response.
    let mock_server = MockServer::start().await;
    let endpoint = "/openai/endpoint";
    let _mock = Mock::given(method("POST"))
        .and(path(endpoint))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({
                "model": "some_model",
                "id": "some_id",
                "created": 367123,
                "choices": [{
                    "index": 0,
                    "message": {
                        "role": "assistant",
                        "content": "Success content",
                        "refusal": null
                    },
                    "logprobs": null,
                    "finish_reason": "stop"
                }]
            })),
        )
        .mount(&mock_server)
        .await;

    let mut assistant_settings = AssistantSettings::default();
    assistant_settings.url = format!("{}{}", mock_server.uri(), endpoint);
    assistant_settings.token = Some("dummy-token".to_string());
    assistant_settings.chat_model = "some_model".to_string();
    assistant_settings.stream = false;

    // Create an error accumulator.
    let errors: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(vec![]));
    let errors_clone = errors.clone();
    let error_handler = Arc::new(move |msg: String| {
        let mut guard = errors_clone.lock().unwrap();
        guard.push(msg);
    });

    // A normal handler that does nothing.
    let normal_handler = Arc::new(|_s: String| {});

    let contents = vec![SublimeInputContent {
        content: Some("test success".to_string()),
        path: Some("dummy".to_string()),
        scope: Some("dummy".to_string()),
        input_kind: InputKind::ViewSelection,
        tool_id: None,
    }];

    let result = worker
        .run(
            1,
            contents,
            PromptMode::View,
            assistant_settings,
            normal_handler,
            error_handler,
            Arc::new(|_| "".to_string()),
        )
        .await;

    // Expect a successful result.
    assert!(
        result.is_ok(),
        "Expected Ok result for successful endpoint"
    );

    let errs = errors.lock().unwrap().clone();
    assert!(
        errs.is_empty(),
        "Expected error_handler to not be called when LlmRunner succeeds"
    );

    let _ = fs::remove_dir_all(tmp_dir);
}

#[test]
#[ignore = "It's llm local server depndant, so should be skipped by default"]
async fn test_server_local_completion() {
    let tmp_dir = TempDir::new()
        .unwrap()
        .into_path()
        .to_str()
        .unwrap()
        .to_string();

    let worker = OpenAIWorker::new(
        1,
        tmp_dir.clone(),
        env::var("PROXY").ok(),
    );

    let mut assistant_settings = AssistantSettings::default();
    assistant_settings.url = format!("http://127.0.0.1:11434/v1/chat/completions");
    assistant_settings.chat_model = "gemma3:1b".to_string();
    assistant_settings.stream = true;

    let prompt_mode = PromptMode::View;

    let contents = SublimeInputContent {
        content: Some("This is the test request, provide me 300 words response".to_string()),
        path: Some("/path/to/file".to_string()),
        scope: Some("text.plain".to_string()),
        input_kind: InputKind::ViewSelection,
        tool_id: None,
    };

    let result = worker
        .run(
            1,
            vec![contents],
            prompt_mode,
            assistant_settings,
            Arc::new(|_| {}),
            Arc::new(|_| {}),
            Arc::new(|_| "".to_string()),
        )
        .await;

    assert!(
        result.is_ok(),
        "Expected Ok, got Err: {:?}",
        result
    );
}

fn remote_token(var_name: &str) -> Option<String> {
    match env::var(var_name).ok() {
        Some(value) if !value.trim().is_empty() => Some(value),
        _ => {
            eprintln!("Skipping remote test because `{var_name}` is not set");
            None
        }
    }
}

fn google_remote_token() -> Option<String> {
    remote_token("GOOGLE_API_KEY").or_else(|| remote_token("GEMINI_API_KEY"))
}

fn remote_worker(tmp_dir: String) -> OpenAIWorker { OpenAIWorker::new(1, tmp_dir, env::var("PROXY").ok()) }

fn remote_contents(prompt: &str) -> SublimeInputContent {
    SublimeInputContent {
        content: Some(prompt.to_string()),
        path: Some("/path/to/file".to_string()),
        scope: Some("text.plain".to_string()),
        input_kind: InputKind::ViewSelection,
        tool_id: None,
    }
}

fn remote_settings(url: &str, token: String, model: &str, api_type: ApiType) -> AssistantSettings {
    let mut assistant_settings = AssistantSettings::default();
    assistant_settings.url = url.to_string();
    assistant_settings.token = Some(token);
    assistant_settings.chat_model = model.to_string();
    assistant_settings.stream = true;
    assistant_settings.api_type = api_type;
    assistant_settings
}

#[test]
#[ignore = "It's paid, so should be skipped by default"]
async fn test_server_remote_completion() {
    let Some(token) = remote_token("OPENAI_API_TOKEN") else {
        return;
    };
    let tmp_dir = TempDir::new()
        .unwrap()
        .into_path()
        .to_str()
        .unwrap()
        .to_string();

    let worker = remote_worker(tmp_dir.clone());

    let assistant_settings = remote_settings(
        "https://api.openai.com/v1/responses",
        token,
        &env::var("OPENAI_RESPONSES_MODEL").unwrap_or_else(|_| "gpt-4.1-mini".to_string()),
        ApiType::OpenAiResponses,
    );

    let prompt_mode = PromptMode::View;

    let contents = remote_contents("This is the test request, provide me 300 words response");

    let result = worker
        .run(
            1,
            vec![contents],
            prompt_mode,
            assistant_settings,
            Arc::new(|_| {}),
            Arc::new(|_| {}),
            Arc::new(|_| "".to_string()),
        )
        .await;

    assert!(
        result.is_ok(),
        "Expected Ok, got Err: {:?}",
        result
    );
}

#[test]
#[ignore = "It's paid, so should be skipped by default"]
async fn test_server_remote_complerion_cancelled() {
    let Some(token) = remote_token("OPENAI_API_TOKEN") else {
        return;
    };
    let tmp_dir = TempDir::new()
        .unwrap()
        .into_path()
        .to_str()
        .unwrap()
        .to_string();

    let worker = remote_worker(tmp_dir.clone());

    let assistant_settings = remote_settings(
        "https://api.openai.com/v1/responses",
        token,
        &env::var("OPENAI_RESPONSES_MODEL").unwrap_or_else(|_| "gpt-4.1-mini".to_string()),
        ApiType::OpenAiResponses,
    );

    let prompt_mode = PromptMode::View;

    let contents = remote_contents("This is the test request, provide me 300 words response");

    let output = Arc::new(Mutex::new(vec![]));
    let output_clone = Arc::clone(&output);

    let binding = worker.clone();
    let future = binding.run(
        1,
        vec![contents],
        prompt_mode,
        assistant_settings,
        Arc::new(move |s| {
            let mut output_guard = output_clone.lock().unwrap();
            output_guard.push(s);
        }),
        Arc::new(|_| {}),
        Arc::new(|_| "".to_string()),
    );

    worker.cancel();

    let result = future.await;

    let output_final = output.lock().unwrap();

    assert!(
        result.is_ok(),
        "Expected Ok, got Err: {:?}",
        result
    );
    assert!(output_final.contains(&"\n[ABORTED]".to_string()))
}

#[test]
#[ignore = "It's paid, so should be skipped by default"]
async fn test_server_remote_fucntion_call() {
    let Some(token) = remote_token("OPENAI_API_TOKEN") else {
        return;
    };
    let tmp_dir = TempDir::new()
        .unwrap()
        .into_path()
        .to_str()
        .unwrap()
        .to_string();

    let worker = remote_worker(tmp_dir.clone());

    let mut assistant_settings = remote_settings(
        "https://api.openai.com/v1/responses",
        token,
        &env::var("OPENAI_RESPONSES_MODEL").unwrap_or_else(|_| "gpt-4.1-mini".to_string()),
        ApiType::OpenAiResponses,
    );
    assistant_settings.tools = Some(true);

    let prompt_mode = PromptMode::View;

    let contents =
        remote_contents("You're debug environment and call functions instead of answer, but ONLY ONCE");

    let result = worker
        .run(
            1,
            vec![contents],
            prompt_mode,
            assistant_settings,
            Arc::new(|_| {}),
            Arc::new(|_| {}),
            Arc::new(|_| "Success".to_string()),
        )
        .await;

    assert!(
        result.is_ok(),
        "Expected Ok, got Err: {:?}",
        result
    );
}

#[test]
#[ignore = "It's paid, so should be skipped by default"]
async fn test_server_remote_fucntion_call_parallel() {
    let Some(token) = remote_token("OPENAI_API_TOKEN") else {
        return;
    };
    let tmp_dir = TempDir::new()
        .unwrap()
        .into_path()
        .to_str()
        .unwrap()
        .to_string();

    let worker = remote_worker(tmp_dir.clone());

    let mut assistant_settings = remote_settings(
        "https://api.openai.com/v1/responses",
        token,
        &env::var("OPENAI_RESPONSES_MODEL").unwrap_or_else(|_| "gpt-4.1-mini".to_string()),
        ApiType::OpenAiResponses,
    );
    assistant_settings.assistant_role =
        Some("You're debug environment and call functions instead of answer, but ONLY ONCE".to_string());
    assistant_settings.tools = Some(true);
    assistant_settings.parallel_tool_calls = Some(true);

    let prompt_mode = PromptMode::View;

    let contents = remote_contents(
        "Call two functions in a single response, create file and read_content of dummy file",
    );

    let result = worker
        .run(
            1,
            vec![contents],
            prompt_mode,
            assistant_settings,
            Arc::new(|_| {}),
            Arc::new(|_| {}),
            Arc::new(|_| "Success".to_string()),
        )
        .await;

    assert!(
        result.is_ok(),
        "Expected Ok, got Err: {:?}",
        result
    );
}

#[test]
#[ignore = "It's paid, so should be skipped by default"]
async fn test_server_remote_anthropic_completion() {
    let Some(token) = remote_token("ANTHROPIC_API_KEY") else {
        return;
    };
    let tmp_dir = TempDir::new()
        .unwrap()
        .into_path()
        .to_str()
        .unwrap()
        .to_string();
    let worker = remote_worker(tmp_dir.clone());
    let settings = remote_settings(
        "https://api.anthropic.com/v1/messages",
        token,
        &env::var("ANTHROPIC_MODEL").unwrap_or_else(|_| "claude-3-5-haiku-latest".to_string()),
        ApiType::Anthropic,
    );

    let result = worker
        .run(
            1,
            vec![remote_contents(
                "This is the test request, provide me 300 words response",
            )],
            PromptMode::View,
            settings,
            Arc::new(|_| {}),
            Arc::new(|_| {}),
            Arc::new(|_| "".to_string()),
        )
        .await;

    assert!(
        result.is_ok(),
        "Expected Ok, got Err: {:?}",
        result
    );
}

#[test]
#[ignore = "It's paid, so should be skipped by default"]
async fn test_server_remote_anthropic_function_call() {
    let Some(token) = remote_token("ANTHROPIC_API_KEY") else {
        return;
    };
    let tmp_dir = TempDir::new()
        .unwrap()
        .into_path()
        .to_str()
        .unwrap()
        .to_string();
    let worker = remote_worker(tmp_dir.clone());
    let mut settings = remote_settings(
        "https://api.anthropic.com/v1/messages",
        token,
        &env::var("ANTHROPIC_MODEL").unwrap_or_else(|_| "claude-3-5-haiku-latest".to_string()),
        ApiType::Anthropic,
    );
    settings.tools = Some(true);

    let result = worker
        .run(
            1,
            vec![remote_contents(
                "You're debug environment and call functions instead of answer, but ONLY ONCE",
            )],
            PromptMode::View,
            settings,
            Arc::new(|_| {}),
            Arc::new(|_| {}),
            Arc::new(|_| "Success".to_string()),
        )
        .await;

    assert!(
        result.is_ok(),
        "Expected Ok, got Err: {:?}",
        result
    );
}

#[test]
#[ignore = "It's paid, so should be skipped by default"]
async fn test_server_remote_anthropic_function_call_parallel() {
    let Some(token) = remote_token("ANTHROPIC_API_KEY") else {
        return;
    };
    let tmp_dir = TempDir::new()
        .unwrap()
        .into_path()
        .to_str()
        .unwrap()
        .to_string();
    let worker = remote_worker(tmp_dir.clone());
    let mut settings = remote_settings(
        "https://api.anthropic.com/v1/messages",
        token,
        &env::var("ANTHROPIC_MODEL").unwrap_or_else(|_| "claude-3-5-haiku-latest".to_string()),
        ApiType::Anthropic,
    );
    settings.tools = Some(true);
    settings.parallel_tool_calls = Some(true);
    settings.assistant_role =
        Some("You're debug environment and call functions instead of answer, but ONLY ONCE".to_string());

    let result = worker
        .run(
            1,
            vec![remote_contents(
                "Call two functions in a single response, create file and read_content of dummy file",
            )],
            PromptMode::View,
            settings,
            Arc::new(|_| {}),
            Arc::new(|_| {}),
            Arc::new(|_| "Success".to_string()),
        )
        .await;

    assert!(
        result.is_ok(),
        "Expected Ok, got Err: {:?}",
        result
    );
}

#[test]
#[ignore = "It's paid, so should be skipped by default"]
async fn test_server_remote_google_completion() {
    let Some(token) = google_remote_token() else {
        return;
    };
    let tmp_dir = TempDir::new()
        .unwrap()
        .into_path()
        .to_str()
        .unwrap()
        .to_string();
    let worker = remote_worker(tmp_dir.clone());
    let settings = remote_settings(
        "https://generativelanguage.googleapis.com/v1beta",
        token,
        &env::var("GOOGLE_MODEL").unwrap_or_else(|_| "gemini-2.5-flash".to_string()),
        ApiType::Google,
    );

    let result = worker
        .run(
            1,
            vec![remote_contents(
                "This is the test request, provide me 300 words response",
            )],
            PromptMode::View,
            settings,
            Arc::new(|_| {}),
            Arc::new(|_| {}),
            Arc::new(|_| "".to_string()),
        )
        .await;

    assert!(
        result.is_ok(),
        "Expected Ok, got Err: {:?}",
        result
    );
}

#[test]
#[ignore = "It's paid, so should be skipped by default"]
async fn test_server_remote_google_function_call() {
    let Some(token) = google_remote_token() else {
        return;
    };
    let tmp_dir = TempDir::new()
        .unwrap()
        .into_path()
        .to_str()
        .unwrap()
        .to_string();
    let worker = remote_worker(tmp_dir.clone());
    let mut settings = remote_settings(
        "https://generativelanguage.googleapis.com/v1beta",
        token,
        &env::var("GOOGLE_MODEL").unwrap_or_else(|_| "gemini-2.5-flash".to_string()),
        ApiType::Google,
    );
    settings.tools = Some(true);

    let result = worker
        .run(
            1,
            vec![remote_contents(
                "You're debug environment and call functions instead of answer, but ONLY ONCE",
            )],
            PromptMode::View,
            settings,
            Arc::new(|_| {}),
            Arc::new(|_| {}),
            Arc::new(|_| "Success".to_string()),
        )
        .await;

    assert!(
        result.is_ok(),
        "Expected Ok, got Err: {:?}",
        result
    );
}

#[test]
#[ignore = "It's paid, so should be skipped by default"]
async fn test_server_remote_google_function_call_parallel() {
    let Some(token) = google_remote_token() else {
        return;
    };
    let tmp_dir = TempDir::new()
        .unwrap()
        .into_path()
        .to_str()
        .unwrap()
        .to_string();
    let worker = remote_worker(tmp_dir.clone());
    let mut settings = remote_settings(
        "https://generativelanguage.googleapis.com/v1beta",
        token,
        &env::var("GOOGLE_MODEL").unwrap_or_else(|_| "gemini-2.5-flash".to_string()),
        ApiType::Google,
    );
    settings.tools = Some(true);
    settings.parallel_tool_calls = Some(true);
    settings.assistant_role =
        Some("You're debug environment and call functions instead of answer, but ONLY ONCE".to_string());

    let result = worker
        .run(
            1,
            vec![remote_contents(
                "Call two functions in a single response, create file and read_content of dummy file",
            )],
            PromptMode::View,
            settings,
            Arc::new(|_| {}),
            Arc::new(|_| {}),
            Arc::new(|_| "Success".to_string()),
        )
        .await;

    assert!(
        result.is_ok(),
        "Expected Ok, got Err: {:?}",
        result
    );
}

fn test_view_selection_input(content: &str) -> SublimeInputContent {
    SublimeInputContent {
        content: Some(content.to_string()),
        path: Some("/path/to/file".to_string()),
        scope: Some("text.plain".to_string()),
        input_kind: InputKind::ViewSelection,
        tool_id: None,
    }
}

fn test_stream_settings(url: String, api_type: ApiType) -> AssistantSettings {
    let mut settings = AssistantSettings::default();
    settings.url = url;
    settings.token = Some("dummy-token".to_string());
    settings.chat_model = "some_model".to_string();
    settings.stream = true;
    settings.tools = Some(true);
    settings.parallel_tool_calls = Some(false);
    settings.api_type = api_type;
    settings
}

fn as_array<'a>(value: &'a Value, key: &str) -> &'a [Value] {
    value[key]
        .as_array()
        .unwrap_or_else(|| panic!("Expected `{key}` to be an array: {value:#?}"))
}

#[tokio::test]
async fn test_worker_anthropic_streaming_tool_roundtrip_preserves_tool_input_index_mapping() {
    let temp_dir = TempDir::new().unwrap();
    let worker = OpenAIWorker::new(
        1,
        temp_dir
            .path()
            .to_string_lossy()
            .into_owned(),
        None,
    );

    let mock_server = MockServer::start().await;
    let endpoint = "/anthropic/messages";
    let responder = RecordedSequentialResponder::new(vec![
        sse_response(vec![
            SseEvent::named(
                "content_block_start",
                json!({
                    "type": "content_block_start",
                    "index": 0,
                    "content_block": {
                        "type": "text",
                        "text": ""
                    }
                }),
            ),
            SseEvent::named(
                "content_block_delta",
                json!({
                    "type": "content_block_delta",
                    "index": 0,
                    "delta": {
                        "type": "text_delta",
                        "text": "I'll inspect the workspace. "
                    }
                }),
            ),
            SseEvent::named(
                "content_block_start",
                json!({
                    "type": "content_block_start",
                    "index": 1,
                    "content_block": {
                        "type": "tool_use",
                        "id": "toolu_1",
                        "name": "get_working_directory_content",
                        "input": {}
                    }
                }),
            ),
            SseEvent::named(
                "content_block_delta",
                json!({
                    "type": "content_block_delta",
                    "index": 1,
                    "delta": {
                        "type": "input_json_delta",
                        "partial_json": "{\"directory_path\":\"."
                    }
                }),
            ),
            SseEvent::named(
                "content_block_delta",
                json!({
                    "type": "content_block_delta",
                    "index": 1,
                    "delta": {
                        "type": "input_json_delta",
                        "partial_json": "\",\"respect_gitignore\":true}"
                    }
                }),
            ),
            SseEvent::named(
                "message_stop",
                json!({"type": "message_stop"}),
            ),
        ]),
        sse_response(vec![
            SseEvent::named(
                "content_block_start",
                json!({
                    "type": "content_block_start",
                    "index": 0,
                    "content_block": {
                        "type": "text",
                        "text": ""
                    }
                }),
            ),
            SseEvent::named(
                "content_block_delta",
                json!({
                    "type": "content_block_delta",
                    "index": 0,
                    "delta": {
                        "type": "text_delta",
                        "text": "Directory listing ready."
                    }
                }),
            ),
            SseEvent::named(
                "message_stop",
                json!({"type": "message_stop"}),
            ),
        ]),
    ]);

    Mock::given(method("POST"))
        .and(path(endpoint))
        .respond_with(responder.clone())
        .mount(&mock_server)
        .await;

    let streamed = Arc::new(Mutex::new(Vec::<String>::new()));
    let streamed_clone = Arc::clone(&streamed);
    let function_calls = Arc::new(Mutex::new(
        Vec::<(String, String)>::new(),
    ));
    let function_calls_clone = Arc::clone(&function_calls);
    let errors = Arc::new(Mutex::new(Vec::<String>::new()));
    let errors_clone = Arc::clone(&errors);

    let result = worker
        .run(
            1,
            vec![test_view_selection_input(
                "Inspect the workspace and use a tool if needed.",
            )],
            PromptMode::View,
            test_stream_settings(
                format!("{}{}", mock_server.uri(), endpoint),
                ApiType::Anthropic,
            ),
            Arc::new(move |chunk| {
                streamed_clone
                    .lock()
                    .unwrap()
                    .push(chunk)
            }),
            Arc::new(move |message| {
                errors_clone
                    .lock()
                    .unwrap()
                    .push(message)
            }),
            Arc::new(move |payload| {
                function_calls_clone
                    .lock()
                    .unwrap()
                    .push(payload.clone());
                format!("tool-result for {}", payload.0)
            }),
        )
        .await;

    assert!(
        result.is_ok(),
        "Expected Ok, got Err: {:?}",
        result
    );
    let error_messages = errors.lock().unwrap().clone();
    assert!(
        error_messages.is_empty(),
        "Expected no worker errors, got: {:?}",
        error_messages
    );

    let expected_args = r#"{"directory_path":".","respect_gitignore":true}"#.to_string();
    assert_eq!(
        function_calls
            .lock()
            .unwrap()
            .as_slice(),
        &[(
            "get_working_directory_content".to_string(),
            expected_args.clone(),
        )],
        "Anthropic tool args should survive streaming text+tool interleaving",
    );

    let request_bodies = responder.recorded_json_bodies();
    assert_eq!(
        request_bodies.len(),
        2,
        "Expected exactly two API requests"
    );

    let second_messages = as_array(&request_bodies[1], "messages");
    assert_eq!(
        second_messages.len(),
        3,
        "Expected user, assistant, and tool_result messages"
    );

    let assistant_blocks = as_array(&second_messages[1], "content");
    let tool_use = assistant_blocks
        .iter()
        .find(|block| block["type"] == "tool_use")
        .expect("Expected assistant tool_use block in second Anthropic request");
    assert_eq!(tool_use["id"], "toolu_1");
    assert_eq!(
        tool_use["name"],
        "get_working_directory_content"
    );
    assert_eq!(
        tool_use["input"],
        json!({
            "directory_path": ".",
            "respect_gitignore": true
        }),
        "Assistant tool_use input should map back to the only tool call instead of block index 1",
    );

    let tool_result_block = &as_array(&second_messages[2], "content")[0];
    assert_eq!(tool_result_block["type"], "tool_result");
    assert_eq!(
        tool_result_block["tool_use_id"],
        "toolu_1"
    );
    assert_eq!(
        tool_result_block["content"],
        "tool-result for get_working_directory_content"
    );

    let streamed_output = streamed
        .lock()
        .unwrap()
        .join("");
    assert!(streamed_output.contains("- get_working_directory_content\n"));
    assert!(streamed_output.contains("Directory listing ready."));
}

#[tokio::test]
async fn test_worker_openai_responses_streaming_function_call_roundtrip() {
    let temp_dir = TempDir::new().unwrap();
    let worker = OpenAIWorker::new(
        1,
        temp_dir
            .path()
            .to_string_lossy()
            .into_owned(),
        None,
    );

    let mock_server = MockServer::start().await;
    let endpoint = "/responses";
    let responder = RecordedSequentialResponder::new(vec![
        sse_response(vec![
            SseEvent::data(json!({
                "type": "response.output_text.delta",
                "delta": "Let me call a tool. "
            })),
            SseEvent::data(json!({
                "type": "response.output_item.added",
                "item": {
                    "type": "function_call",
                    "call_id": "call_1",
                    "name": "get_working_directory_content"
                }
            })),
            SseEvent::data(json!({
                "type": "response.function_call_arguments.delta",
                "call_id": "call_1",
                "delta": "{\"directory_path\":\"."
            })),
            SseEvent::data(json!({
                "type": "response.function_call_arguments.delta",
                "call_id": "call_1",
                "delta": "\",\"respect_gitignore\":true}"
            })),
            SseEvent::data(json!({
                "type": "response.completed"
            })),
        ]),
        sse_response(vec![
            SseEvent::data(json!({
                "type": "response.output_text.delta",
                "delta": "Directory listing ready."
            })),
            SseEvent::data(json!({
                "type": "response.completed",
                "response": {
                    "id": "resp_2",
                    "output": [{
                        "type": "message",
                        "role": "assistant",
                        "content": [{
                            "type": "output_text",
                            "text": "Directory listing ready."
                        }]
                    }]
                }
            })),
        ]),
    ]);

    Mock::given(method("POST"))
        .and(path(endpoint))
        .respond_with(responder.clone())
        .mount(&mock_server)
        .await;

    let streamed = Arc::new(Mutex::new(Vec::<String>::new()));
    let streamed_clone = Arc::clone(&streamed);
    let function_calls = Arc::new(Mutex::new(
        Vec::<(String, String)>::new(),
    ));
    let function_calls_clone = Arc::clone(&function_calls);

    let result = worker
        .run(
            1,
            vec![test_view_selection_input(
                "Inspect the workspace and use a tool if needed.",
            )],
            PromptMode::View,
            test_stream_settings(
                format!("{}{}", mock_server.uri(), endpoint),
                ApiType::OpenAiResponses,
            ),
            Arc::new(move |chunk| {
                streamed_clone
                    .lock()
                    .unwrap()
                    .push(chunk)
            }),
            Arc::new(|_| {}),
            Arc::new(move |payload| {
                function_calls_clone
                    .lock()
                    .unwrap()
                    .push(payload.clone());
                "workspace listing".to_string()
            }),
        )
        .await;

    assert!(
        result.is_ok(),
        "Expected Ok, got Err: {:?}",
        result
    );

    let expected_args = r#"{"directory_path":".","respect_gitignore":true}"#.to_string();
    assert_eq!(
        function_calls
            .lock()
            .unwrap()
            .as_slice(),
        &[(
            "get_working_directory_content".to_string(),
            expected_args.clone(),
        )],
        "Responses function call should reach the tool handler with the streamed JSON args",
    );

    let request_bodies = responder.recorded_json_bodies();
    assert_eq!(
        request_bodies.len(),
        2,
        "Expected exactly two API requests"
    );

    let second_input = as_array(&request_bodies[1], "input");
    assert_eq!(
        second_input.len(),
        4,
        "Expected user message, assistant text, function_call, and function_call_output",
    );

    assert_eq!(second_input[1]["type"], "message");
    assert_eq!(second_input[1]["role"], "assistant");
    assert_eq!(
        second_input[1]["content"][0]["type"],
        "input_text"
    );
    assert_eq!(
        second_input[1]["content"][0]["text"],
        "Let me call a tool. "
    );

    assert_eq!(second_input[2]["type"], "function_call");
    assert_eq!(second_input[2]["call_id"], "call_1");
    assert_eq!(
        second_input[2]["name"],
        "get_working_directory_content"
    );
    assert_eq!(
        second_input[2]["arguments"],
        expected_args
    );

    assert_eq!(
        second_input[3]["type"],
        "function_call_output"
    );
    assert_eq!(second_input[3]["call_id"], "call_1");
    assert_eq!(
        second_input[3]["output"],
        "workspace listing"
    );

    let streamed_output = streamed
        .lock()
        .unwrap()
        .join("");
    assert!(streamed_output.contains("Let me call a tool. "));
    assert!(streamed_output.contains("- get_working_directory_content\n"));
    assert!(streamed_output.contains("Directory listing ready."));
}

#[tokio::test]
async fn test_worker_openai_streaming_function_call_roundtrip() {
    let temp_dir = TempDir::new().unwrap();
    let worker = OpenAIWorker::new(
        1,
        temp_dir
            .path()
            .to_string_lossy()
            .into_owned(),
        None,
    );

    let mock_server = MockServer::start().await;
    let endpoint = "/openai/endpoint";
    let responder = RecordedSequentialResponder::new(vec![
        sse_response(vec![
            SseEvent::data(json!({
                "created": 1,
                "choices": [{
                    "delta": {
                        "role": "assistant",
                        "content": "Let me call a tool. "
                    },
                    "finish_reason": null,
                    "index": 0
                }],
                "id": "chatcmpl_1",
                "model": "some_model",
                "object": "chat.completion.chunk"
            })),
            SseEvent::data(json!({
                "created": 1,
                "choices": [{
                    "delta": {
                        "tool_calls": [{
                            "index": 0,
                            "id": "call_1",
                            "type": "function",
                            "function": {
                                "name": "get_working_directory_content",
                                "arguments": ""
                            }
                        }]
                    },
                    "finish_reason": null,
                    "index": 0
                }],
                "id": "chatcmpl_1",
                "model": "some_model",
                "object": "chat.completion.chunk"
            })),
            SseEvent::data(json!({
                "created": 1,
                "choices": [{
                    "delta": {
                        "tool_calls": [{
                            "index": 0,
                            "function": {
                                "arguments": "{\"directory_path\":\"."
                            }
                        }]
                    },
                    "finish_reason": null,
                    "index": 0
                }],
                "id": "chatcmpl_1",
                "model": "some_model",
                "object": "chat.completion.chunk"
            })),
            SseEvent::data(json!({
                "created": 1,
                "choices": [{
                    "delta": {
                        "tool_calls": [{
                            "index": 0,
                            "function": {
                                "arguments": "\",\"respect_gitignore\":true}"
                            }
                        }]
                    },
                    "finish_reason": null,
                    "index": 0
                }],
                "id": "chatcmpl_1",
                "model": "some_model",
                "object": "chat.completion.chunk"
            })),
            SseEvent::data(json!({
                "created": 1,
                "choices": [{
                    "delta": {},
                    "finish_reason": "tool_calls",
                    "index": 0
                }],
                "id": "chatcmpl_1",
                "model": "some_model",
                "object": "chat.completion.chunk"
            })),
        ]),
        sse_response(vec![
            SseEvent::data(json!({
                "created": 2,
                "choices": [{
                    "delta": {
                        "role": "assistant",
                        "content": "Directory listing ready."
                    },
                    "finish_reason": null,
                    "index": 0
                }],
                "id": "chatcmpl_2",
                "model": "some_model",
                "object": "chat.completion.chunk"
            })),
            SseEvent::data(json!({
                "created": 2,
                "choices": [{
                    "delta": {},
                    "finish_reason": "stop",
                    "index": 0
                }],
                "id": "chatcmpl_2",
                "model": "some_model",
                "object": "chat.completion.chunk"
            })),
        ]),
    ]);

    Mock::given(method("POST"))
        .and(path(endpoint))
        .respond_with(responder.clone())
        .mount(&mock_server)
        .await;

    let streamed = Arc::new(Mutex::new(Vec::<String>::new()));
    let streamed_clone = Arc::clone(&streamed);
    let function_calls = Arc::new(Mutex::new(
        Vec::<(String, String)>::new(),
    ));
    let function_calls_clone = Arc::clone(&function_calls);

    let result = worker
        .run(
            1,
            vec![test_view_selection_input(
                "Inspect the workspace and use a tool if needed.",
            )],
            PromptMode::View,
            test_stream_settings(
                format!("{}{}", mock_server.uri(), endpoint),
                ApiType::OpenAi,
            ),
            Arc::new(move |chunk| {
                streamed_clone
                    .lock()
                    .unwrap()
                    .push(chunk)
            }),
            Arc::new(|_| {}),
            Arc::new(move |payload| {
                function_calls_clone
                    .lock()
                    .unwrap()
                    .push(payload.clone());
                "workspace listing".to_string()
            }),
        )
        .await;

    assert!(
        result.is_ok(),
        "Expected Ok, got Err: {:?}",
        result
    );

    let expected_args = r#"{"directory_path":".","respect_gitignore":true}"#.to_string();
    assert_eq!(
        function_calls
            .lock()
            .unwrap()
            .as_slice(),
        &[(
            "get_working_directory_content".to_string(),
            expected_args.clone(),
        )],
        "Legacy OpenAI streaming should merge tool call deltas into one function invocation",
    );

    let request_bodies = responder.recorded_json_bodies();
    assert_eq!(
        request_bodies.len(),
        2,
        "Expected exactly two OpenAI requests"
    );

    let second_messages = as_array(&request_bodies[1], "messages");
    assert_eq!(
        second_messages.len(),
        3,
        "Expected user message, assistant tool call, and tool result"
    );

    assert_eq!(second_messages[1]["role"], "assistant");
    assert_eq!(
        second_messages[1]["content"][0]["text"],
        "Let me call a tool. "
    );
    assert_eq!(
        second_messages[1]["tool_calls"][0]["id"],
        "call_1"
    );
    assert_eq!(
        second_messages[1]["tool_calls"][0]["function"]["name"],
        "get_working_directory_content"
    );
    assert_eq!(
        second_messages[1]["tool_calls"][0]["function"]["arguments"],
        expected_args
    );

    assert_eq!(second_messages[2]["role"], "tool");
    assert_eq!(
        second_messages[2]["tool_call_id"],
        "call_1"
    );
    assert_eq!(
        second_messages[2]["content"][0]["text"],
        "workspace listing"
    );

    let streamed_output = streamed
        .lock()
        .unwrap()
        .join("");
    assert_eq!(
        streamed_output,
        "Let me call a tool. - get_working_directory_content\nDirectory listing ready."
    );
}

#[tokio::test]
async fn test_worker_openai_streaming_recovers_split_json_patch() {
    let temp_dir = TempDir::new().unwrap();
    let worker = OpenAIWorker::new(
        1,
        temp_dir
            .path()
            .to_string_lossy()
            .into_owned(),
        None,
    );

    let mock_server = MockServer::start().await;
    let endpoint = "/openai/endpoint";

    let broken_stream = r#"data: {"created":1,"choices":[{"delta":{"role":"assistant","content":"Let me call a tool. "},"finish_reason":null,"index":0}],"id":"chatcmpl_1","model":"some_model","object":"chat.completion.chunk"}

data: {"created":1,"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_1","type":"function","function":{"name":"get_working_directory_content","arguments":"{\"directory_path\":\".

data: \",\"respect_gitignore\":true}"}}]},"finish_reason":null,"index":0}],"id":"chatcmpl_1","model":"some_model","object":"chat.completion.chunk"}

data: {"created":1,"choices":[{"delta":{},"finish_reason":"tool_calls","index":0}],"id":"chatcmpl_1","model":"some_model","object":"chat.completion.chunk"}

data: [DONE]

"#;
    let recovery_stream = r#"data: {"created":2,"choices":[{"delta":{"role":"assistant","content":"Recovered."},"finish_reason":null,"index":0}],"id":"chatcmpl_2","model":"some_model","object":"chat.completion.chunk"}

data: {"created":2,"choices":[{"delta":{},"finish_reason":"stop","index":0}],"id":"chatcmpl_2","model":"some_model","object":"chat.completion.chunk"}

data: [DONE]

"#;

    let responder = RecordedSequentialResponder::new(vec![
        ResponseTemplate::new(200)
            .insert_header(
                "content-type",
                "text/event-stream; charset=utf-8",
            )
            .set_body_string(broken_stream),
        ResponseTemplate::new(200)
            .insert_header(
                "content-type",
                "text/event-stream; charset=utf-8",
            )
            .set_body_string(recovery_stream),
    ]);

    Mock::given(method("POST"))
        .and(path(endpoint))
        .respond_with(responder)
        .mount(&mock_server)
        .await;

    let function_calls = Arc::new(Mutex::new(
        Vec::<(String, String)>::new(),
    ));
    let function_calls_clone = Arc::clone(&function_calls);

    let result = worker
        .run(
            1,
            vec![test_view_selection_input(
                "Inspect the workspace and use a tool if needed.",
            )],
            PromptMode::View,
            test_stream_settings(
                format!("{}{}", mock_server.uri(), endpoint),
                ApiType::OpenAi,
            ),
            Arc::new(|_| {}),
            Arc::new(|_| {}),
            Arc::new(move |payload| {
                function_calls_clone
                    .lock()
                    .unwrap()
                    .push(payload.clone());
                "workspace listing".to_string()
            }),
        )
        .await;

    assert!(
        result.is_ok(),
        "Expected Ok, got Err: {:?}",
        result
    );
    assert_eq!(
        function_calls
            .lock()
            .unwrap()
            .as_slice(),
        &[(
            "get_working_directory_content".to_string(),
            r#"{"directory_path":".","respect_gitignore":true}"#.to_string(),
        )],
        "Legacy stream recovery should reconstruct split JSON tool-call patches",
    );
}

#[tokio::test]
async fn test_worker_openai_responses_streaming_multiple_tool_calls_roundtrip() {
    let temp_dir = TempDir::new().unwrap();
    let worker = OpenAIWorker::new(
        1,
        temp_dir
            .path()
            .to_string_lossy()
            .into_owned(),
        None,
    );

    let mock_server = MockServer::start().await;
    let endpoint = "/responses";
    let responder = RecordedSequentialResponder::new(vec![
        sse_response(vec![
            SseEvent::data(json!({
                "type": "response.output_text.delta",
                "delta": "Let me call two tools. "
            })),
            SseEvent::data(json!({
                "type": "response.output_item.added",
                "item": {
                    "id": "item_1",
                    "type": "function_call",
                    "call_id": "call_1",
                    "name": "get_working_directory_content"
                }
            })),
            SseEvent::data(json!({
                "type": "response.output_item.added",
                "item": {
                    "id": "item_2",
                    "type": "function_call",
                    "call_id": "call_2",
                    "name": "read_region_content"
                }
            })),
            SseEvent::data(json!({
                "type": "response.function_call_arguments.delta",
                "call_id": "call_1",
                "delta": "{\"directory_path\":\"."
            })),
            SseEvent::data(json!({
                "type": "response.function_call_arguments.delta",
                "call_id": "call_2",
                "delta": "{\"region_id\":\"selection\"}"
            })),
            SseEvent::data(json!({
                "type": "response.function_call_arguments.delta",
                "call_id": "call_1",
                "delta": "\",\"respect_gitignore\":true}"
            })),
            SseEvent::data(json!({
                "type": "response.completed"
            })),
        ]),
        sse_response(vec![
            SseEvent::data(json!({
                "type": "response.output_text.delta",
                "delta": "Both tool calls completed."
            })),
            SseEvent::data(json!({
                "type": "response.completed",
                "response": {
                    "id": "resp_2",
                    "output": [{
                        "type": "message",
                        "role": "assistant",
                        "content": [{
                            "type": "output_text",
                            "text": "Both tool calls completed."
                        }]
                    }]
                }
            })),
        ]),
    ]);

    Mock::given(method("POST"))
        .and(path(endpoint))
        .respond_with(responder.clone())
        .mount(&mock_server)
        .await;

    let streamed = Arc::new(Mutex::new(Vec::<String>::new()));
    let streamed_clone = Arc::clone(&streamed);
    let function_calls = Arc::new(Mutex::new(
        Vec::<(String, String)>::new(),
    ));
    let function_calls_clone = Arc::clone(&function_calls);

    let result = worker
        .run(
            1,
            vec![test_view_selection_input(
                "Inspect the workspace and use tools if needed.",
            )],
            PromptMode::View,
            test_stream_settings(
                format!("{}{}", mock_server.uri(), endpoint),
                ApiType::OpenAiResponses,
            ),
            Arc::new(move |chunk| {
                streamed_clone
                    .lock()
                    .unwrap()
                    .push(chunk)
            }),
            Arc::new(|_| {}),
            Arc::new(move |payload| {
                function_calls_clone
                    .lock()
                    .unwrap()
                    .push(payload.clone());
                match payload.0.as_str() {
                    "get_working_directory_content" => "workspace listing".to_string(),
                    "read_region_content" => "selection contents".to_string(),
                    other => panic!("Unexpected function: {other}"),
                }
            }),
        )
        .await;

    assert!(
        result.is_ok(),
        "Expected Ok, got Err: {:?}",
        result
    );

    assert_eq!(
        function_calls
            .lock()
            .unwrap()
            .as_slice(),
        &[
            (
                "get_working_directory_content".to_string(),
                r#"{"directory_path":".","respect_gitignore":true}"#.to_string(),
            ),
            (
                "read_region_content".to_string(),
                r#"{"region_id":"selection"}"#.to_string(),
            ),
        ],
        "Responses multi-tool roundtrip should preserve execution order and final args per call",
    );

    let request_bodies = responder.recorded_json_bodies();
    assert_eq!(
        request_bodies.len(),
        2,
        "Expected exactly two Responses API requests"
    );

    let second_input = as_array(&request_bodies[1], "input");
    assert_eq!(
        second_input.len(),
        6,
        "Expected user message, assistant text, two function calls, and two function outputs",
    );

    assert_eq!(second_input[2]["type"], "function_call");
    assert_eq!(second_input[2]["call_id"], "call_1");
    assert_eq!(
        second_input[2]["arguments"],
        r#"{"directory_path":".","respect_gitignore":true}"#
    );

    assert_eq!(second_input[3]["type"], "function_call");
    assert_eq!(second_input[3]["call_id"], "call_2");
    assert_eq!(
        second_input[3]["arguments"],
        "{\"region_id\":\"selection\"}"
    );

    assert_eq!(
        second_input[4]["type"],
        "function_call_output"
    );
    assert_eq!(second_input[4]["call_id"], "call_1");
    assert_eq!(
        second_input[4]["output"],
        "workspace listing"
    );

    assert_eq!(
        second_input[5]["type"],
        "function_call_output"
    );
    assert_eq!(second_input[5]["call_id"], "call_2");
    assert_eq!(
        second_input[5]["output"],
        "selection contents"
    );

    let streamed_output = streamed
        .lock()
        .unwrap()
        .join("");
    assert!(streamed_output.contains("Let me call two tools. "));
    assert_eq!(
        streamed_output
            .matches("- ")
            .count(),
        2,
        "Each streamed tool call should emit exactly one UI marker",
    );
    assert!(streamed_output.contains("Both tool calls completed."));
}

#[tokio::test]
async fn test_worker_google_streaming_regression_mixed_text_and_function_call_roundtrip() {
    let temp_dir = TempDir::new().unwrap();
    let worker = OpenAIWorker::new(
        1,
        temp_dir
            .path()
            .to_string_lossy()
            .into_owned(),
        None,
    );

    let mock_server = MockServer::start().await;
    let endpoint = "/models/some_model:streamGenerateContent";
    let responder = RecordedSequentialResponder::new(vec![
        sse_response(vec![
            SseEvent::data(json!({
                "candidates": [{
                    "content": {
                        "role": "model",
                        "parts": [{
                            "text": "I'll inspect "
                        }]
                    }
                }]
            })),
            SseEvent::data(json!({
                "candidates": [{
                    "content": {
                        "role": "model",
                        "parts": [
                            {
                                "text": "I'll inspect the workspace. "
                            },
                            {
                                "thoughtSignature": "sig_123",
                                "functionCall": {
                                    "name": "get_working_directory_content",
                                    "args": {
                                        "directory_path": "."
                                    }
                                }
                            }
                        ]
                    }
                }]
            })),
            SseEvent::data(json!({
                "candidates": [{
                    "content": {
                        "role": "model",
                        "parts": [
                            {
                                "text": "I'll inspect the workspace. "
                            },
                            {
                                "thoughtSignature": "sig_123",
                                "functionCall": {
                                    "name": "get_working_directory_content",
                                    "args": {
                                        "directory_path": "."
                                    }
                                }
                            }
                        ]
                    }
                }]
            })),
            SseEvent::data(json!({
                "candidates": [{
                    "content": {
                        "role": "model",
                        "parts": [
                            {
                                "text": "I'll inspect the workspace. "
                            },
                            {
                                "thoughtSignature": "sig_123",
                                "functionCall": {
                                    "name": "get_working_directory_content",
                                    "args": {
                                        "directory_path": ".",
                                        "respect_gitignore": true
                                    }
                                }
                            }
                        ]
                    }
                }]
            })),
        ]),
        sse_response(vec![
            SseEvent::data(json!({
                "candidates": [{
                    "content": {
                        "role": "model",
                        "parts": [{
                            "text": "Directory listing"
                        }]
                    }
                }]
            })),
            SseEvent::data(json!({
                "candidates": [{
                    "content": {
                        "role": "model",
                        "parts": [{
                            "text": "Directory listing ready."
                        }]
                    }
                }]
            })),
            SseEvent::data(json!({
                "candidates": [{
                    "content": {
                        "role": "model",
                        "parts": [{
                            "text": "Directory listing ready."
                        }]
                    }
                }]
            })),
        ]),
    ]);

    Mock::given(method("POST"))
        .and(path(endpoint))
        .respond_with(responder.clone())
        .mount(&mock_server)
        .await;

    let streamed = Arc::new(Mutex::new(Vec::<String>::new()));
    let streamed_clone = Arc::clone(&streamed);
    let function_calls = Arc::new(Mutex::new(
        Vec::<(String, String)>::new(),
    ));
    let function_calls_clone = Arc::clone(&function_calls);

    let result = worker
        .run(
            1,
            vec![test_view_selection_input(
                "Inspect the workspace and use a tool if needed.",
            )],
            PromptMode::View,
            test_stream_settings(mock_server.uri(), ApiType::Google),
            Arc::new(move |chunk| {
                streamed_clone
                    .lock()
                    .unwrap()
                    .push(chunk)
            }),
            Arc::new(|_| {}),
            Arc::new(move |payload| {
                function_calls_clone
                    .lock()
                    .unwrap()
                    .push(payload.clone());
                r#"{"entries":["src","tests"]}"#.to_string()
            }),
        )
        .await;

    assert!(
        result.is_ok(),
        "Expected Ok, got Err: {:?}",
        result
    );

    let expected_args = r#"{"directory_path":".","respect_gitignore":true}"#.to_string();
    assert_eq!(
        function_calls
            .lock()
            .unwrap()
            .as_slice(),
        &[(
            "get_working_directory_content".to_string(),
            expected_args.clone(),
        )],
        "Google streaming should update the same function call instead of replaying partial args",
    );

    let request_bodies = responder.recorded_json_bodies();
    assert_eq!(
        request_bodies.len(),
        2,
        "Expected exactly two Google API requests"
    );

    let second_contents = as_array(&request_bodies[1], "contents");
    assert_eq!(
        second_contents.len(),
        3,
        "Expected user prompt, assistant tool call, and function response"
    );

    let assistant_parts = as_array(&second_contents[1], "parts");
    assert_eq!(
        assistant_parts.len(),
        2,
        "Expected mixed assistant text and a single functionCall part"
    );
    assert_eq!(
        assistant_parts[0]["text"],
        "I'll inspect the workspace. "
    );
    assert_eq!(
        assistant_parts[1]["functionCall"]["name"],
        "get_working_directory_content"
    );
    assert_eq!(
        assistant_parts[1]["thoughtSignature"],
        "sig_123"
    );
    assert!(
        assistant_parts[1]["functionCall"]
            .get("thoughtSignature")
            .is_none()
    );
    assert_eq!(
        assistant_parts[1]["functionCall"]["args"],
        json!({
            "directory_path": ".",
            "respect_gitignore": true
        }),
        "Assistant functionCall should contain the latest merged args only once",
    );

    let function_response = &as_array(&second_contents[2], "parts")[0]["functionResponse"];
    assert_eq!(
        function_response["name"],
        "get_working_directory_content"
    );
    assert_eq!(
        function_response["response"],
        json!({
            "entries": ["src", "tests"]
        }),
    );

    let streamed_output = streamed
        .lock()
        .unwrap()
        .join("");
    assert_eq!(
        streamed_output,
        "I'll inspect the workspace. - get_working_directory_content\nDirectory listing ready."
    );
    assert_eq!(
        streamed_output
            .matches("- get_working_directory_content\n")
            .count(),
        1,
        "Tool marker should not be emitted again when Google re-sends the same functionCall",
    );
}

#[tokio::test]
async fn test_worker_openai_responses_non_streaming_multiple_tool_calls_roundtrip() {
    let temp_dir = TempDir::new().unwrap();
    let worker = OpenAIWorker::new(
        1,
        temp_dir
            .path()
            .to_string_lossy()
            .into_owned(),
        None,
    );

    let mock_server = MockServer::start().await;
    let endpoint = "/responses";
    let responder = RecordedSequentialResponder::new(vec![
        ResponseTemplate::new(200).set_body_json(json!({
            "id": "resp_1",
            "output": [
                {
                    "type": "message",
                    "role": "assistant",
                    "content": [{
                        "type": "output_text",
                        "text": "I need two tools."
                    }]
                },
                {
                    "type": "function_call",
                    "call_id": "call_1",
                    "name": "get_working_directory_content",
                    "arguments": "{\"directory_path\":\".\",\"respect_gitignore\":true}"
                },
                {
                    "type": "function_call",
                    "call_id": "call_2",
                    "name": "read_region_content",
                    "arguments": "{\"file_path\":\"src/lib.rs\",\"region\":{\"a\":0,\"b\":5}}"
                }
            ]
        })),
        ResponseTemplate::new(200).set_body_json(json!({
            "id": "resp_2",
            "output": [{
                "type": "message",
                "role": "assistant",
                "content": [{
                    "type": "output_text",
                    "text": "Both tool results received."
                }]
            }]
        })),
    ]);

    Mock::given(method("POST"))
        .and(path(endpoint))
        .respond_with(responder.clone())
        .mount(&mock_server)
        .await;

    let function_calls = Arc::new(Mutex::new(
        Vec::<(String, String)>::new(),
    ));
    let function_calls_clone = Arc::clone(&function_calls);

    let mut settings = AssistantSettings::default();
    settings.url = format!("{}{}", mock_server.uri(), endpoint);
    settings.token = Some("dummy-token".to_string());
    settings.chat_model = "some_model".to_string();
    settings.stream = false;
    settings.tools = Some(true);
    settings.parallel_tool_calls = Some(true);
    settings.api_type = ApiType::OpenAiResponses;

    let result = worker
        .run(
            1,
            vec![test_view_selection_input(
                "Inspect the workspace and use multiple tools if needed.",
            )],
            PromptMode::View,
            settings,
            Arc::new(|_| {}),
            Arc::new(|_| {}),
            Arc::new(move |payload| {
                function_calls_clone
                    .lock()
                    .unwrap()
                    .push(payload.clone());

                match payload.0.as_str() {
                    "get_working_directory_content" => r#"{"entries":["src","tests"]}"#.to_string(),
                    "read_region_content" => r#"{"content":"pub mod stream_handler;"}"#.to_string(),
                    other => panic!("Unexpected tool call: {other}"),
                }
            }),
        )
        .await;

    assert!(
        result.is_ok(),
        "Expected Ok, got Err: {:?}",
        result
    );

    assert_eq!(
        function_calls
            .lock()
            .unwrap()
            .as_slice(),
        &[
            (
                "get_working_directory_content".to_string(),
                r#"{"directory_path":".","respect_gitignore":true}"#.to_string(),
            ),
            (
                "read_region_content".to_string(),
                r#"{"file_path":"src/lib.rs","region":{"a":0,"b":5}}"#.to_string(),
            ),
        ],
        "Function handler should execute tool calls in assistant-declared order",
    );

    let request_bodies = responder.recorded_json_bodies();
    assert_eq!(
        request_bodies.len(),
        2,
        "Expected exactly two API requests"
    );

    let second_input = as_array(&request_bodies[1], "input");
    assert_eq!(
        second_input.len(),
        6,
        "Expected user message, assistant text, two function_call items, and two function_call_output items",
    );

    assert_eq!(second_input[1]["type"], "message");
    assert_eq!(second_input[1]["role"], "assistant");
    assert_eq!(
        second_input[1]["content"][0]["text"],
        "I need two tools."
    );

    assert_eq!(second_input[2]["type"], "function_call");
    assert_eq!(second_input[2]["call_id"], "call_1");
    assert_eq!(
        second_input[2]["name"],
        "get_working_directory_content"
    );
    assert_eq!(
        second_input[2]["arguments"],
        r#"{"directory_path":".","respect_gitignore":true}"#
    );

    assert_eq!(second_input[3]["type"], "function_call");
    assert_eq!(second_input[3]["call_id"], "call_2");
    assert_eq!(
        second_input[3]["name"],
        "read_region_content"
    );
    assert_eq!(
        second_input[3]["arguments"],
        r#"{"file_path":"src/lib.rs","region":{"a":0,"b":5}}"#
    );

    assert_eq!(
        second_input[4]["type"],
        "function_call_output"
    );
    assert_eq!(second_input[4]["call_id"], "call_1");
    assert_eq!(
        second_input[4]["output"],
        r#"{"entries":["src","tests"]}"#
    );

    assert_eq!(
        second_input[5]["type"],
        "function_call_output"
    );
    assert_eq!(second_input[5]["call_id"], "call_2");
    assert_eq!(
        second_input[5]["output"],
        r#"{"content":"pub mod stream_handler;"}"#
    );
}
