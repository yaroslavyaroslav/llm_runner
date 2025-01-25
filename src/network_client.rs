use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use anyhow::Result;
use futures_util::StreamExt;
use reqwest::{
    header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE},
    Client,
    Proxy,
    Request,
};
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};
use tokio::sync::{mpsc::Sender, Mutex};

use crate::{
    openai_network_types::OpenAICompletionRequest,
    types::{AssistantSettings, CacheEntry, SublimeInputContent},
};

#[derive(Clone)]
pub struct NetworkClient {
    client: Client,
    headers: HeaderMap,
}

impl NetworkClient {
    pub(crate) fn new(proxy: Option<String>) -> Self {
        let mut headers = HeaderMap::new();
        headers.insert(
            CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );

        let client = proxy
            .and_then(|proxy_line| Proxy::all(proxy_line).ok())
            .map(|proxy| {
                Client::builder()
                    .proxy(proxy)
                    .build()
                    .unwrap_or_default()
            })
            .unwrap_or_default();

        Self { client, headers }
    }

    pub(crate) fn prepare_payload(
        &self,
        settings: AssistantSettings,
        cache_entries: Vec<CacheEntry>,
        sublime_inputs: Vec<SublimeInputContent>,
    ) -> Result<String> {
        let internal_messages = OpenAICompletionRequest::create_openai_completion_request(
            settings,
            cache_entries,
            sublime_inputs,
        );

        Ok(serde_json::to_string(
            &internal_messages,
        )?)
    }

    pub(crate) fn prepare_request(
        &self,
        settings: AssistantSettings,
        json_payload: String,
    ) -> Result<Request> {
        let url = settings.url;
        let mut headers = self.headers.clone();
        if let Some(token) = settings.token {
            let auth_header = format!("Bearer {}", token);
            let auth_header = HeaderValue::from_str(&auth_header)?;
            headers.insert(AUTHORIZATION, auth_header);
        }

        Ok(self
            .client
            .post(url)
            .headers(headers)
            .body(json_payload)
            .build()?)
    }

    pub async fn execute_request<T>(
        &self,
        request: Request,
        sender: Arc<Mutex<Sender<String>>>,
        cancel_flag: Arc<AtomicBool>,
        stream: bool,
    ) -> Result<T>
    where
        T: DeserializeOwned,
    {
        let response = self
            .client
            .execute(request)
            .await?;

        let mut composable_response = serde_json::json!({});

        if stream {
            if response.status().is_success() {
                let mut stream = response.bytes_stream();

                while let Some(chunk) = stream.next().await {
                    let chunk = chunk?;

                    let data = String::from_utf8_lossy(&chunk);

                    for line in data.lines() {
                        if line.trim() == "data: [DONE]" {
                            break;
                        }

                        if let Some(stripped) = line
                            .trim_start()
                            .strip_prefix("data: ")
                        {
                            let json_value: serde_json::Value = serde_json::from_str(stripped)?;

                            merge_json(&mut composable_response, &json_value);

                            // TODO: To add "[ABORTED]" to history as well on break
                            if let Some(content) = json_value
                                .get("choices")
                                .and_then(|c| c.as_array())
                                .and_then(|arr| arr.first())
                                .and_then(|first| first.as_object())
                                .and_then(|fisr_object| obtain_delta(fisr_object))
                            {
                                let cloned_sender = Arc::clone(&sender);

                                cloned_sender
                                    .lock()
                                    .await
                                    .send(content)
                                    .await
                                    .ok();
                            }
                        }
                    }
                    if cancel_flag.load(Ordering::SeqCst) {
                        break;
                    }
                }
                if cancel_flag.load(Ordering::SeqCst) {
                    let cloned_sender = Arc::clone(&sender);

                    cloned_sender
                        .lock()
                        .await
                        .send("\n[ABORTED]".to_string())
                        .await
                        .ok();
                }

                drop(sender);

                Ok(serde_json::from_value::<T>(
                    composable_response,
                )?)
            } else {
                Err(anyhow::anyhow!(format!(
                    "Request failed with status: {}",
                    response.status()
                ))
                .into())
            }
        } else if response.status().is_success() {
            let json_body = response
                .json::<Value>()
                .await?;

            if let Some(content) = json_body
                .clone()
                .get("choices")
                .and_then(|c| c.as_array())
                .and_then(|arr| arr.first())
                .and_then(|first| first.as_object())
                .and_then(|fisr_object| fisr_object.get("message"))
                .and_then(|message| message.as_object())
                .and_then(|m| m.get("content"))
                .and_then(|c| c.as_str())
            {
                let cloned_sender = Arc::clone(&sender);
                let string = content.to_string();

                cloned_sender
                    .lock()
                    .await
                    .send(string)
                    .await
                    .ok();
            }
            Ok(serde_json::from_value::<T>(json_body)?)
        } else {
            Err(anyhow::anyhow!(format!(
                "Request failed with status: {}",
                response.status()
            ))
            .into())
        }
    }
}

/// This function is actually handles the SSE stream from the llm
/// There are two cases handled here so far:
///  - llm text answer: the `"content"` field is getting concantinated during
///    this call
///  - llm function call: the `"tool_calls"[0]."function"."arguments"` field is
///    getting concantinated during this call
///
/// The main assumption here is that the response can never be mixed
/// to contain both `"content"` and `"tool_calls"` in a single stream.
fn merge_json(base: &mut Value, addition: &Value) {
    match (base, addition) {
        (Value::Object(base_map), Value::Object(addition_map)) => {
            for (key, value) in addition_map {
                match key.as_str() {
                    "content" => {
                        if value.is_null() {
                            eprintln!("Skipping null 'content' field");
                            continue;
                        }
                        if let Some(Value::String(existing_value)) = base_map.get_mut(key) {
                            if let Value::String(addition_value) = value {
                                existing_value.push_str(addition_value);
                            }
                        }
                    }
                    "tool_calls" => {
                        if let (Some(base_array), Some(addition_array)) = (
                            base_map
                                .get_mut(key)
                                .and_then(|v| v.as_array_mut()),
                            value.as_array(),
                        ) {
                            merge_tool_calls(base_array, addition_array.to_vec());
                        } else {
                            base_map.insert(key.to_string(), value.clone());
                        }
                    }
                    _ => {
                        merge_json(
                            base_map
                                .entry(key)
                                .or_insert(Value::Null),
                            value,
                        )
                    }
                }
            }
        }
        (Value::Array(base_array), Value::Array(addition_array)) => {
            merge_json(&mut base_array[0], &addition_array[0]);
        }
        (base, addition) => {
            *base = addition.clone();
        }
    }
}

fn merge_tool_calls(base_array: &mut [Value], addition_array: Vec<Value>) {
    for (base_item, addition_item) in base_array
        .iter_mut()
        .zip(addition_array)
    {
        merge_tool_call(base_item, &addition_item);
    }
}

fn merge_tool_call(base_item: &mut Value, addition_item: &Value) {
    if let (Some(base_args), Some(addition_args)) = (
        base_item
            .get_mut("function")
            .and_then(|f| f.get_mut("arguments")),
        addition_item
            .get("function")
            .and_then(|f| f.get("arguments")),
    ) {
        if let Some(base_args_str) = base_args.as_str() {
            if let Some(addition_args_str) = addition_args.as_str() {
                *base_args = serde_json::json!(format!(
                    "{}{}",
                    base_args_str, addition_args_str
                ));
            } else {
                *base_args = addition_args.clone();
            }
        } else {
            *base_args = addition_args.clone();
        }
    }
}

/// This function extracts a plain string for streaming it into UI
/// This is either `"content"` field (the actual answer of the llm) or
/// a function call, where it is the `"arguments"` the one that actually
/// streams.
///
/// Thus there's low sense of showing the exact arguments of the call to a user
/// only `"tool_calls"[0]."function"."name"` streams in the latter case here
/// (it's a one shot).
fn obtain_delta(map: &Map<String, Value>) -> Option<String> {
    if let Some(delta) = map.get("delta") {
        if let Some(content) = delta
            .get("content")
            .and_then(|c| c.as_str())
        {
            return Some(content.to_string());
        }
        if let Some(function_name) = delta
            .get("tool_calls")
            .and_then(|v| v.as_array())
            .and_then(|array| array.first())
            .and_then(|first_item| first_item.get("function"))
            .and_then(|function| function.get("name"))
        {
            return function_name
                .as_str()
                .map(|s| s.to_string());
        }
    }

    for value in map.values() {
        return value
            .as_object()
            .and_then(|map| obtain_delta(map));
    }

    None
}

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};
    use tokio::{sync::mpsc, test};
    use wiremock::{
        matchers::{header, method},
        MockServer,
        ResponseTemplate,
    };

    use super::*;
    use crate::types::InputKind;

    #[derive(Serialize, Deserialize, Debug)]
    struct TestMessage {
        role: String,
        content: String,
    }

    #[derive(Serialize, Deserialize, Debug)]
    struct TestResponse {
        id: String,
        object: String,
    }

    #[test]
    async fn test_is_sync_and_send() {
        fn is_sync<T: Sync>() {}
        fn is_send<T: Send>() {}

        is_sync::<NetworkClient>();
        is_send::<NetworkClient>();
    }

    #[test]
    async fn test_prepare_payload() {
        let client = NetworkClient::new(None);
        let settings = AssistantSettings::default();

        let cache_entries = vec![];
        let sublime_inputs = vec![SublimeInputContent {
            content: Some("content".to_string()),
            path: None,
            scope: None,
            input_kind: InputKind::ViewSelection,
            tool_id: None,
        }];

        let payload = client
            .prepare_payload(settings, cache_entries, sublime_inputs)
            .unwrap();

        let payload_json: serde_json::Value = serde_json::from_str(&payload).unwrap();
        let expected_payload = serde_json::json!({
            "messages": [
                {
                    "content": [
                        {
                            "text": "content",
                            "type": "text",
                        }
                    ],
                    "role": "user",
                }
            ],
            "stream": true,
            "model": "gpt-4o-mini",
        });

        assert_eq!(payload_json, expected_payload);
    }

    #[test]
    async fn test_prepare_request() {
        let client = NetworkClient::new(None);
        let mut settings = AssistantSettings::default();
        let url = "https://models.inference.ai.azure.com/some/path".to_string();
        settings.url = url.clone();

        let cache_entries = vec![];
        let sublime_inputs = vec![SublimeInputContent {
            content: Some("content".to_string()),
            path: None,
            scope: None,
            input_kind: InputKind::ViewSelection,
            tool_id: None,
        }];

        let payload = client
            .prepare_payload(
                settings.clone(),
                cache_entries,
                sublime_inputs,
            )
            .unwrap();

        let request = client
            .prepare_request(settings.clone(), payload)
            .unwrap();

        assert_eq!(request.url().as_str(), url);
    }

    #[tokio::test]
    async fn test_execute_response() {
        let mock_server = MockServer::start().await;
        let _mock = wiremock::Mock::given(method("POST"))
            .and(header(
                CONTENT_TYPE.as_str(),
                "application/json",
            ))
            .respond_with(
                ResponseTemplate::new(200).set_body_string("{\"id\": \"1\", \"object\": \"object\"}"),
            )
            .mount(&mock_server)
            .await;

        let client = NetworkClient::new(None);
        let mut settings = AssistantSettings::default();
        settings.url = mock_server.uri();
        settings.stream = false;

        let cache_entries = vec![];
        let sublime_inputs = vec![SublimeInputContent {
            content: Some("content".to_string()),
            path: None,
            scope: None,
            input_kind: InputKind::ViewSelection,
            tool_id: None,
        }];

        let payload = client
            .prepare_payload(
                settings.clone(),
                cache_entries,
                sublime_inputs,
            )
            .unwrap();

        let request = client
            .prepare_request(settings.clone(), payload)
            .unwrap();

        let (tx, _) = mpsc::channel(10);

        let response: Result<TestResponse, _> = client
            .execute_request(
                request,
                Arc::new(Mutex::new(tx)),
                Arc::new(AtomicBool::new(false)),
                settings.stream,
            )
            .await;

        assert_eq!(
            response.as_ref().unwrap().id,
            "1".to_string()
        );
        assert_eq!(
            response.unwrap().object,
            "object".to_string()
        );
    }

    #[tokio::test]
    async fn test_sse_streaming() {
        let mock_server = MockServer::start().await;

        // SSE content for testing
        let sse_data = r#"
        data: {"choices":[{"delta":{"content":"The","role":"assistant","tool_calls":null},"finish_reason":null,"index":0}],"created":1734374933,"id":"cmpl-9775b1b7-0746-470e-a541-e0cc8f73bcce","model":"Llama-3.3-70B-Instruct","object":"chat.completion.chunk","usage":null}

        data: {"choices":[{"delta":{"content":" ","role":"assistant","tool_calls":null},"finish_reason":null,"index":0}],"created":1734374933,"id":"cmpl-9775b1b7-0746-470e-a541-e0cc8f73bcce","model":"Llama-3.3-70B-Instruct","object":"chat.completion.chunk","usage":null}

        data: {"choices":[{"delta":{"content":"202","role":"assistant","tool_calls":null},"finish_reason":null,"index":0}],"created":1734374933,"id":"cmpl-9775b1b7-0746-470e-a541-e0cc8f73bcce","model":"Llama-3.3-70B-Instruct","object":"chat.completion.chunk","usage":null}

        data: [DONE]


        "#;
        let _mock = wiremock::Mock::given(method("POST"))
            .and(header(
                CONTENT_TYPE.as_str(),
                "application/json",
            ))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header(
                        CONTENT_TYPE.as_str(),
                        "text/event-stream; charset=utf-8",
                    )
                    .set_body_string(sse_data),
            )
            .mount(&mock_server)
            .await;

        let client = NetworkClient::new(None);
        let mut settings = AssistantSettings::default();
        settings.url = mock_server.uri();

        let cache_entries = vec![];
        let sublime_inputs = vec![SublimeInputContent {
            content: Some("content".to_string()),
            path: None,
            scope: None,
            input_kind: InputKind::ViewSelection,
            tool_id: None,
        }];

        let payload = client
            .prepare_payload(
                settings.clone(),
                cache_entries,
                sublime_inputs,
            )
            .unwrap();

        let request = client
            .prepare_request(settings.clone(), payload)
            .unwrap();

        let (tx, mut rx) = mpsc::channel(10);

        let result = client
            .execute_request::<Map<String, Value>>(
                request,
                Arc::new(Mutex::new(tx)),
                Arc::new(AtomicBool::new(false)),
                settings.stream,
            )
            .await;

        let mut events = vec![];
        while let Some(data) = rx.recv().await {
            events.push(data);
        }

        let content = result
            .unwrap()
            .get("choices")
            .unwrap()
            .as_array()
            .unwrap()
            .get(0)
            .unwrap()
            .get("delta")
            .unwrap()
            .get("content")
            .unwrap()
            .as_str()
            .unwrap()
            .to_string();

        assert_eq!(events, vec!["The", " ", "202"]);
        assert_eq!(content, events.join(""));
    }

    #[tokio::test]
    async fn test_sse_tool_calls_streaming() {
        let mock_server = MockServer::start().await;

        let sse_data = r#"
        data: {"id":"8f18fa2f381e5b8e-VIE","object":"chat.completion.chunk","created":1734124608,"model":"model","choices":[{"index":0,"delta":{"role":"assistant","content":null},"logprobs":null,"finish_reason":null}]}

        data: {"id":"8f18fa2f381e5b8e-VIE","object":"chat.completion.chunk","created":1734124608,"model":"model","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call_hozqwzmegi9la14u8wmizj35","type":"function","function":{"name":"create_file","arguments":""}}]},"logprobs":null,"finish_reason":null}]}

        data: {"id":"8f18fa2f381e5b8e-VIE","object":"chat.completion.chunk","created":1734124608,"model":"model","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\""}}]},"logprobs":null,"finish_reason":null}]}

        data: {"id":"8f18fa2f381e5b8e-VIE","object":"chat.completion.chunk","created":1734124608,"model":"model","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":"file\""}}]},"logprobs":null,"finish_reason":null}]}

        data: {"id":"8f18fa2f381e5b8e-VIE","object":"chat.completion.chunk","created":1734124608,"model":"model","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":": "}}]},"logprobs":null,"finish_reason":null}]}

        data: {"id":"8f18fa2f381e5b8e-VIE","object":"chat.completion.chunk","created":1734124608,"model":"model","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":"\"some\"}"}}]},"logprobs":null,"finish_reason":null}]}

        data: [DONE]
        "#;

        let _mock = wiremock::Mock::given(method("POST"))
            .and(header(
                CONTENT_TYPE.as_str(),
                "application/json",
            ))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header(
                        CONTENT_TYPE.as_str(),
                        "text/event-stream; charset=utf-8",
                    )
                    .set_body_string(sse_data),
            )
            .mount(&mock_server)
            .await;

        let client = NetworkClient::new(None);
        let mut settings = AssistantSettings::default();
        settings.url = mock_server.uri();

        let payload = "dummy payload";
        let request = client
            .prepare_request(settings.clone(), payload.to_string())
            .unwrap();

        let (tx, mut rx) = mpsc::channel(10);

        let result = client
            .execute_request::<Map<String, Value>>(
                request,
                Arc::new(Mutex::new(tx)),
                Arc::new(AtomicBool::new(false)),
                settings.stream,
            )
            .await;

        let mut function_name = vec![];
        while let Some(data) = rx.recv().await {
            function_name.push(data);
        }

        let binding = result.unwrap();
        let tool_calls_array = binding
            .get("choices")
            .unwrap()
            .as_array()
            .unwrap()
            .get(0)
            .unwrap()
            .get("delta")
            .unwrap()
            .get("tool_calls")
            .unwrap()
            .as_array()
            .unwrap();

        assert_eq!(function_name.join(""), "create_file");

        assert_eq!(
            tool_calls_array[0]
                .get("function")
                .unwrap()
                .get("name")
                .unwrap()
                .as_str()
                .unwrap(),
            "create_file"
        );

        assert_eq!(
            tool_calls_array[0]
                .get("function")
                .unwrap()
                .get("arguments")
                .unwrap()
                .as_str()
                .unwrap(),
            "{\"file\": \"some\"}"
        );
    }

    #[tokio::test]
    async fn test_tool_calls_non_streaming() {
        let mock_server = MockServer::start().await;

        let non_streaming_data = r#"
            {
                "delta": {
                    "tool_calls": [
                        {
                            "index": 0,
                            "id": "tool_1",
                            "type": "function_call",
                            "function": {
                                "name": "fetch_data",
                                "arguments": "{ \"param1\": \"value1\" }"
                            }
                        }
                    ]
                }
            }
        "#;

        let _mock = wiremock::Mock::given(method("POST"))
            .and(header(
                CONTENT_TYPE.as_str(),
                "application/json",
            ))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header(
                        CONTENT_TYPE.as_str(),
                        "application/json",
                    )
                    .set_body_string(non_streaming_data),
            )
            .mount(&mock_server)
            .await;

        let client = NetworkClient::new(None);
        let mut settings = AssistantSettings::default();
        settings.url = mock_server.uri();
        settings.stream = false;

        let payload = "dummy payload";
        let request = client
            .prepare_request(settings.clone(), payload.to_string())
            .unwrap();

        let (tx, _) = mpsc::channel(10);

        let result: Map<String, Value> = client
            .execute_request::<Map<String, Value>>(
                request,
                Arc::new(Mutex::new(tx)),
                Arc::new(AtomicBool::new(false)),
                settings.stream,
            )
            .await
            .unwrap();

        let tool_calls_array = result
            .get("delta")
            .unwrap()
            .get("tool_calls")
            .unwrap()
            .as_array()
            .unwrap();

        assert_eq!(
            tool_calls_array[0]
                .get("function")
                .unwrap()
                .get("name")
                .unwrap()
                .as_str()
                .unwrap(),
            "fetch_data"
        );

        assert_eq!(
            tool_calls_array[0]
                .get("function")
                .unwrap()
                .get("arguments")
                .unwrap()
                .as_str()
                .unwrap(),
            r#"{ "param1": "value1" }"#
        );
    }

    #[tokio::test]
    async fn test_network_client_abort() {
        let mock_server = MockServer::start().await;

        // SSE content for testing
        let sse_data = r#"
        data: {"choices":[{"delta":{"content":"The","role":"assistant","tool_calls":null},"finish_reason":null,"index":0}],"created":1734374933,"id":"cmpl-9775b1b7-0746-470e-a541-e0cc8f73bcce","model":"Llama-3.3-70B-Instruct","object":"chat.completion.chunk","usage":null}

        data: {"choices":[{"delta":{"content":" ","role":"assistant","tool_calls":null},"finish_reason":null,"index":0}],"created":1734374933,"id":"cmpl-9775b1b7-0746-470e-a541-e0cc8f73bcce","model":"Llama-3.3-70B-Instruct","object":"chat.completion.chunk","usage":null}

        data: {"choices":[{"delta":{"content":"FAIL","role":"assistant","tool_calls":null},"finish_reason":null,"index":0}],"created":1734374933,"id":"cmpl-9775b1b7-0746-470e-a541-e0cc8f73bcce","model":"Llama-3.3-70B-Instruct","object":"chat.completion.chunk","usage":null}

        data: [DONE]

        "#;

        wiremock::Mock::given(method("POST"))
            .and(header(
                CONTENT_TYPE.as_str(),
                "application/json",
            ))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header(
                        CONTENT_TYPE.as_str(),
                        "text/event-stream; charset=utf-8",
                    )
                    .set_body_string(sse_data),
            )
            .mount(&mock_server)
            .await;

        let settings = AssistantSettings {
            name: "Test Assistant".to_string(),
            output_mode: crate::types::PromptMode::Phantom,
            chat_model: "gpt-4o-mini".to_string(),
            url: mock_server.uri(),
            token: None,
            assistant_role: None,
            temperature: None,
            max_tokens: None,
            max_completion_tokens: None,
            top_p: None,
            frequency_penalty: None,
            presence_penalty: None,
            tools: None,
            parallel_tool_calls: None,
            stream: true,
            advertisement: false,
        };

        let cancel_flag = Arc::new(AtomicBool::new(false));

        let cancel_flag_clone = Arc::clone(&cancel_flag);

        let (tx, mut rx) = mpsc::channel(10);

        let task = tokio::spawn(async move {
            let client = NetworkClient::new(None);
            let payload = "dummy payload";
            let request = client
                .prepare_request(settings.clone(), payload.to_string())
                .unwrap();

            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

            let response = client
                .execute_request::<Map<String, Value>>(
                    request,
                    Arc::new(Mutex::new(tx)),
                    cancel_flag_clone,
                    settings.stream,
                )
                .await;

            match response {
                Ok(_) => println!("Request completed successfully!"),
                Err(e) => println!("Request failed: {:?}", e),
            }
        });

        cancel_flag.store(true, Ordering::SeqCst);

        let mut output = vec![];
        while let Some(string) = rx.recv().await {
            output.push(string);
        }

        let _ = task.await;

        assert_eq!(output, vec!["The", "\n[ABORTED]"]); // Only the first chunk should proceed
    }
}
