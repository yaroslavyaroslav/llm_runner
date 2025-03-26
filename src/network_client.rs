use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use anyhow::Result;
use eventsource_stream::Eventsource;
use futures_util::StreamExt;
use log::debug;
use reqwest::{
    header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION, CONTENT_TYPE},
    Client,
    Proxy,
    Request,
};
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};
use tokio::{
    sync::{mpsc::Sender, Mutex},
    time::timeout,
};

use crate::{
    openai_network_types::{
        ErrorResponse,
        OpenAICompletionRequest,
        OpenAIErrorContainer,
        OtherErrorContainer,
    },
    types::{AssistantSettings, CacheEntry, SublimeInputContent},
};

#[derive(Clone)]
pub struct NetworkClient {
    client: Client,
    headers: HeaderMap,
    timeout: usize,
}

impl NetworkClient {
    pub(crate) fn new(proxy: Option<String>, timeout: usize) -> Self {
        let mut headers = HeaderMap::new();
        headers.insert(
            CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        headers.insert(
            ACCEPT,
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

        Self {
            client,
            headers,
            timeout,
        }
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

        let composable_response = Arc::new(Mutex::new(serde_json::json!({})));

        #[cfg(debug_assertions)]
        use crate::logger;
        #[cfg(debug_assertions)]
        let _ = logger::setup_logger("/tmp/rsvr_log.log");

        if stream {
            if response.status().is_success() {
                let mut stream = response
                    .bytes_stream()
                    .eventsource();
                let mut buffer = String::new();

                loop {
                    match timeout(
                        Duration::from_secs(self.timeout as u64),
                        stream.next(),
                    )
                    .await
                    {
                        Ok(Some(Ok(event))) => {
                            // ...
                            let composable = Arc::clone(&composable_response);
                            let cloned_sender = Arc::clone(&sender);

                            debug!("received json: {:?}", event.data);
                            if let Ok(combined) = serde_json::from_str::<Value>(&buffer) {
                                if combined
                                    .as_object()
                                    .and_then(|obj| obj.get("usage"))
                                    .and_then(|value| value.as_object())
                                    .is_some()
                                {
                                    break; // fuckers from together never gives a fuck about to send [DONE] token for R1
                                }

                                let _ = Self::handle_json(composable, combined, cloned_sender).await;
                                buffer.clear();
                            } else {
                                match serde_json::from_str::<Value>(&event.data) {
                                    Ok(json_value) => {
                                        if json_value
                                            .as_object()
                                            .and_then(|obj| obj.get("usage"))
                                            .and_then(|value| value.as_object())
                                            .is_some()
                                        {
                                            break; // fuckers from together never gives a fuck about to send [DONE] token for R1
                                        }

                                        let _ = Self::handle_json(
                                            composable,
                                            json_value.clone(),
                                            cloned_sender,
                                        )
                                        .await;
                                    }
                                    Err(e) => {
                                        if e.is_eof() {
                                            buffer.push_str(&event.data);
                                        }
                                    }
                                }
                            }

                            if event.data.contains("[DONE]") || cancel_flag.load(Ordering::SeqCst) {
                                break;
                            }
                        }
                        Ok(Some(Err(e))) => {
                            debug!("Error of accessing event: {:?}", e);
                            break;
                        }
                        Ok(None) => {
                            // Stream is exhausted
                            debug!("Stream is exhausted");
                            break;
                        }
                        Err(_) => {
                            // Timeout exceeded
                            debug!("Stream is stalled");
                            let cloned_sender = Arc::clone(&sender);

                            cloned_sender
                                .lock()
                                .await
                                .send("\n[STALLED]".to_string())
                                .await
                                .ok();
                            break; // fuckers from together can stall stream for more than 10 secs for R1
                        }
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
                debug!(
                    "composable_response: {:?}",
                    composable_response
                );

                let result = dbg!(composable_response)
                    .lock()
                    .await
                    .take();

                Ok(serde_json::from_value::<T>(result)?)
            } else {
                debug!("some_error: {:?}", composable_response);
                let status = &response.status();
                let error_body_string = response.text().await?;
                let error_object: ErrorResponse =
                    serde_json::from_str::<OpenAIErrorContainer>(&error_body_string)
                        .map(ErrorResponse::OpenAI)
                        .or_else(|_| {
                            serde_json::from_str::<OtherErrorContainer>(&error_body_string)
                                .map(ErrorResponse::Other)
                        })
                        .unwrap_or(ErrorResponse::Message(
                            error_body_string,
                        ));

                Err(anyhow::anyhow!(format!(
                    "Request failed with status: {}, the error: {}",
                    status,
                    error_object.message()
                )))
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
            )))
        }
    }

    async fn handle_json(
        composable_response: Arc<Mutex<serde_json::Value>>,
        json_value: serde_json::Value,
        sender: Arc<Mutex<Sender<String>>>,
    ) -> Result<()> {
        debug!("handle_json: {:?}", json_value);

        let mut result = composable_response
            .lock()
            .await;

        let _ = Self::merge_json(&mut result, &json_value);

        if let Some(content) = json_value
            .get("choices")
            .and_then(|c| c.as_array())
            .and_then(|arr| arr.first())
            .and_then(|first| first.as_object())
            .and_then(Self::obtain_delta)
        {
            debug!("send_json: {:?}", content);
            sender
                .lock()
                .await
                .send(content)
                .await
                .map_err(|e| {
                    anyhow::anyhow!(format!(
                        "Failed to send the data: {}",
                        e
                    ))
                })
        } else {
            Err(anyhow::anyhow!(format!(
                "Object has wrong :",
            )))
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
    fn merge_json(base: &mut Value, addition: &Value) -> Result<()> {
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
                                let _ = Self::merge_tool_calls(base_array, addition_array.to_vec());
                            } else {
                                base_map.insert(key.to_string(), value.clone());
                            }
                        }
                        _ => {
                            let _ = Self::merge_json(
                                base_map
                                    .entry(key)
                                    .or_insert(Value::Null),
                                value,
                            );
                        }
                    }
                }
                Ok(())
            }
            (Value::Array(base_array), Value::Array(addition_array)) => {
                // Previous fallback: if arrays are non-empty, merge the first items.
                if !addition_array.is_empty() && !base_array.is_empty() {
                    let _ = Self::merge_json(&mut base_array[0], &addition_array[0]);
                }
                Ok(())
            }
            (base, addition) => {
                *base = addition.clone();
                Ok(())
            }
        }
    }

    fn merge_tool_calls(base_array: &mut Vec<Value>, addition_array: Vec<Value>) -> Result<()> {
        for addition_item in addition_array {
            // Check for an "index" field.
            if let Some(index_value) = addition_item.get("index") {
                if let Some(index) = index_value.as_u64() {
                    let idx = index as usize;
                    // Ensure the base vector is large enough.
                    if idx >= base_array.len() {
                        base_array.resize_with(idx + 1, || serde_json::json!({}));
                    }
                    // Remove the "index" field so it doesn't persist.
                    let mut trimmed_addition = addition_item.clone();
                    if let Value::Object(ref mut obj) = trimmed_addition {
                        obj.remove("index");
                    }
                    let _ = Self::merge_tool_call(&mut base_array[idx], &trimmed_addition);
                }
            } else {
                // If no index is provided, merge with the first element.
                if !base_array.is_empty() {
                    let _ = Self::merge_tool_call(&mut base_array[0], &addition_item);
                }
            }
        }
        Ok(())
    }

    fn merge_tool_call(base_item: &mut Value, addition_item: &Value) -> Result<()> {
        let base_obj = base_item
            .as_object_mut()
            .expect("Expected base_item to be an object");
        dbg!(&base_obj);
        dbg!(addition_item);

        if let Some(new_args) = addition_item
            .get("function")
            .and_then(|f| f.get("arguments"))
            .and_then(Value::as_str)
        {
            if let Some(base_function_map) = base_obj
                .get_mut("function")
                .and_then(|f| f.as_object_mut())
            {
                let entry = base_function_map
                    .entry("arguments".to_string())
                    .or_insert(Value::String(String::new()));
                if let Value::String(existing_args) = entry {
                    existing_args.push_str(new_args);
                }
            }
        }

        // For "function", "id", and "type", set them from addition_item if they're not already present.
        for key in &["function", "id", "type"] {
            if base_obj.get(*key).is_none() {
                if let Some(val) = addition_item.get(*key) {
                    base_obj.insert((*key).to_string(), val.clone());
                }
            }
        }

        Ok(())
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

        if let Some(value) = map.values().next() {
            return value
                .as_object()
                .and_then(Self::obtain_delta);
        }

        None
    }
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
    use crate::types::{ApiType, InputKind};

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
        let client = NetworkClient::new(None, 10);
        let mut settings = AssistantSettings::default();

        settings.api_type = ApiType::OpenAi;

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
        let client = NetworkClient::new(None, 10);
        let mut settings = AssistantSettings::default();
        settings.api_type = ApiType::OpenAi;
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

        let client = NetworkClient::new(None, 10);
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
    #[ignore = "Unable to perform actual streaming with mock server"]
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

        let client = NetworkClient::new(None, 10);
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

        let content = dbg!(result)
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
    #[ignore = "Unable to perform actual streaming with mock server"]
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

        let client = NetworkClient::new(None, 10);
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

        let binding = dbg!(result).unwrap();
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

        let client = NetworkClient::new(None, 10);
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

    // Cancel definitely working at the point 2700dcb298a3abcd88c62da0b5324be2d2739eb2
    // Seems like is too slow to abort the stream, it could be caused by that previously stream receiving handler
    // started working after the whole remote stream was processed beforehand.
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
            reasoning_effort: None,
            top_p: None,
            frequency_penalty: None,
            presence_penalty: None,
            tools: None,
            parallel_tool_calls: None,
            timeout: 10,
            stream: true,
            advertisement: false,
            api_type: ApiType::OpenAi,
        };

        let cancel_flag = Arc::new(AtomicBool::new(false));

        let cancel_flag_clone = Arc::clone(&cancel_flag);

        let (tx, mut rx) = mpsc::channel(10);

        let task = tokio::spawn(async move {
            let client = NetworkClient::new(None, 10);
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

        assert!(output.contains(&"\n[ABORTED]".to_string()))
    }
}
