use futures_util::StreamExt;
use serde::Serialize;
use serde_json::{Map, Value};
use std::error::Error;

use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use reqwest::{Client, Request};
use serde::de::DeserializeOwned;
use tokio::sync::mpsc;

use crate::types::AssistantSettings;

#[derive(Debug)]
pub enum OpenAIErrors {
    ContextLengthExceededException,
    UnknownException,
    ReqwestError(reqwest::Error),
    InvalidHeaderError(String),
    JsonError(serde_json::Error),
}
pub struct NetworkClient {
    client: Client,
    headers: HeaderMap,
}

impl std::error::Error for OpenAIErrors {}

impl std::fmt::Display for OpenAIErrors {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OpenAIErrors::ContextLengthExceededException => {
                write!(f, "The context length exceeds the limit")
            }
            OpenAIErrors::InvalidHeaderError(err) => write!(f, "Invalid header got passed {}", err),
            OpenAIErrors::UnknownException => write!(f, "An unknown exception occurred"),
            OpenAIErrors::ReqwestError(err) => write!(f, "A reqwest error occurred: {}", err),
            OpenAIErrors::JsonError(err) => write!(f, "A json error occurred: {}", err),
        }
    }
}

impl NetworkClient {
    pub(crate) fn new() -> Self {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("content/json"));

        let client = Client::new();

        Self { client, headers }
    }

    pub(crate) fn prepare_payload<T>(
        &self,
        settings: AssistantSettings,
        messages: Vec<T>,
    ) -> Result<String, OpenAIErrors>
    where
        T: Serialize,
    {
        let internal_messages: Vec<serde_json::Value> = if settings.assistant_role.is_empty() {
            messages
                .into_iter()
                .map(|m| serde_json::to_value(m))
                .collect::<Result<Vec<serde_json::Value>, _>>()
                .map_err(OpenAIErrors::JsonError)?
        } else {
            let mut internal_messages = vec![serde_json::json!({
                "role": "system",
                "content": settings.assistant_role,
            })];
            internal_messages.extend(
                messages
                    .into_iter()
                    .map(|m| serde_json::to_value(m))
                    .collect::<Result<Vec<serde_json::Value>, _>>()
                    .map_err(OpenAIErrors::JsonError)?,
            );
            internal_messages
        };

        serde_json::to_string(&internal_messages).map_err(OpenAIErrors::JsonError)
    }

    pub(crate) fn prepare_request(
        &self,
        settings: AssistantSettings,
        json_payload: String,
    ) -> Result<Request, OpenAIErrors> {
        let url = settings.url.to_string();
        let mut headers = self.headers.clone();
        let auth_header = format!("Bearer {}", settings.token);
        let auth_header = HeaderValue::from_str(&auth_header)
            .map_err(|e| OpenAIErrors::InvalidHeaderError(e.to_string()))?;

        headers.insert(AUTHORIZATION, auth_header);

        self.client
            .post(url)
            .headers(headers)
            .body(json_payload)
            .build()
            .map_err(OpenAIErrors::ReqwestError)
    }

    pub async fn execute_response<T>(
        &self,
        request: Request,
        sender: Option<mpsc::Sender<String>>,
    ) -> Result<T, Box<dyn Error>>
    where
        T: DeserializeOwned,
    {
        let response = self.client.execute(request).await?;

        let mut composable_response = serde_json::json!({});

        if let Some(sender) = sender {
            if response.status().is_success() {
                let mut stream = response.bytes_stream();

                while let Some(chunk) = stream.next().await {
                    let chunk = chunk?;

                    let data = String::from_utf8_lossy(&chunk);

                    for line in data.lines() {
                        if let Some(stripped) = line.trim_start().strip_prefix("data: ") {
                            let tmp_dict: Map<String, Value> = serde_json::from_str(stripped)?;
                            let json_chunk: Value = serde_json::from_str(stripped)?;

                            merge_json(&mut composable_response, &json_chunk);

                            if let Some(content) = obtain_delta(&tmp_dict) {
                                if sender.send(content).await.is_err() {
                                    eprintln!("Failed to send SSE data");
                                }
                            }
                        }
                    }
                }
                drop(sender);

                Ok(serde_json::from_value::<T>(composable_response)?)
            } else {
                Err(format!("Request failed with status: {}", response.status()).into())
            }
        } else if response.status().is_success() {
            let payload = response.json::<T>().await?;
            Ok(payload)
        } else {
            Err(format!("Request failed with status: {}", response.status()).into())
        }
    }
}

/// This function is actually handles the SSE stream from the llm
/// There are two cases handled here so far:
///  - llm text answer: the `"content"` field is getting concantinated during this call
///  - llm function call: the `"tool_calls"[0]."function"."arguments"` field is getting concantinated during this call
///
/// The main assumption here is that the response can never be mixed
/// to contain both `"content"` and `"tool_calls"` in a single stream.
fn merge_json(base: &mut Value, addition: &Value) {
    match (base, addition) {
        (Value::Object(base_map), Value::Object(addition_map)) => {
            for (key, value) in addition_map {
                match key.as_str() {
                    "content" if base_map.contains_key(key) => {
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
                        if let (Some(Value::Array(base_array)), Value::Array(addition_array)) =
                            (base_map.get_mut(key), value)
                        {
                            for (base_item, addition_item) in
                                base_array.iter_mut().zip(addition_array.iter())
                            {
                                if let (
                                    Some(Value::String(base_args)),
                                    Some(Value::String(addition_args)),
                                ) = (
                                    base_item
                                        .get_mut("function")
                                        .and_then(|f| f.get_mut("arguments")),
                                    addition_item
                                        .get("function")
                                        .and_then(|f| f.get("arguments")),
                                ) {
                                    base_args.push_str(addition_args);
                                }
                            }
                        }
                    }
                    _ => merge_json(base_map.entry(key).or_insert(Value::Null), value),
                }
            }
        }
        (Value::Array(base_array), Value::Array(addition_array)) => {
            let mut base_object = base_array[0].clone();
            let additional_object = addition_array[0].clone();
            merge_json(&mut base_object, &additional_object);
        }
        (base, addition) => {
            dbg!(&base);
            *base = addition.clone();
        }
    }
}

/// This function extracts a plain string for streaming it into UI
/// This is either `"content"` field (the actual answer of the llm) or
/// a function call, where it is the `"arguments"` the one that actually streams.
///
/// Thus there's low sense of showing the exact arguments of the call to a user
/// only `"tool_calls"[0]."function"."name"` streams in the latter case here (it's a one shot).
fn obtain_delta(map: &Map<String, Value>) -> Option<String> {
    if let Some(delta) = map.get("delta") {
        if let Some(content) = delta.get("content").and_then(|c| c.as_str()) {
            return Some(content.to_string());
        }
        if let Some(function_name) = delta
            .get("tool_calls")
            .and_then(|v| v.as_array())
            .and_then(|array| array.first())
            .and_then(|first_item| first_item.get("function"))
            .and_then(|function| function.get("name"))
        {
            return function_name.as_str().map(|s| s.to_string());
        }
    }

    for value in map.values() {
        if let Some(nested_map) = value.as_object() {
            if let Some(result) = obtain_delta(nested_map) {
                return Some(result);
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use tokio::test;
    use wiremock::matchers::{header, method};
    use wiremock::{MockServer, ResponseTemplate};

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
    async fn test_prepare_payload() {
        let client = NetworkClient::new();
        let settings = AssistantSettings {
            token: "token".to_string(),
            url: "url".to_string(),
            chat_model: "model".to_string(),
            temperature: 0.5,
            max_tokens: 100,
            max_completion_tokens: 100,
            top_p: 0.5,
            stream: false,
            parallel_tool_calls: false,
            tools: false,
            advertisement: false,
            assistant_role: "".to_string(),
        };

        let messages = vec![TestMessage {
            role: "role".to_string(),
            content: "content".to_string(),
        }];

        let payload = client.prepare_payload(settings, messages).unwrap();

        let expected_payload = serde_json::json!([
            {
                "role": "role",
                "content": "content",
            },
        ])
        .to_string();

        assert_eq!(payload, expected_payload);
    }

    #[tokio::test]
    async fn test_execute_response() {
        let mock_server = MockServer::start().await;
        let _mock = wiremock::Mock::given(method("POST"))
            .and(header(CONTENT_TYPE.as_str(), "content/json"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string("{\"id\": \"1\", \"object\": \"object\"}"),
            )
            .mount(&mock_server)
            .await;

        let client = NetworkClient::new();
        let settings = AssistantSettings {
            token: "token".to_string(),
            url: mock_server.uri(),
            chat_model: "model".to_string(),
            temperature: 0.5,
            max_tokens: 100,
            max_completion_tokens: 100,
            top_p: 0.5,
            stream: false,
            parallel_tool_calls: false,
            tools: false,
            advertisement: false,
            assistant_role: "".to_string(),
        };

        let messages = vec![TestMessage {
            role: "role".to_string(),
            content: "content".to_string(),
        }];

        let payload = client.prepare_payload(settings.clone(), messages).unwrap();

        let request = client.prepare_request(settings, payload).unwrap();

        let response: Result<TestResponse, _> = client.execute_response(request, None).await;

        assert_eq!(response.as_ref().unwrap().id, "1".to_string());
        assert_eq!(response.unwrap().object, "object".to_string());
    }

    #[tokio::test]
    async fn test_sse_streaming() {
        let mock_server = MockServer::start().await;

        // SSE content for testing
        let sse_data = r#"
            data: { "delta": { "content": "Hello world!" } }

            data: { "delta": { "content": "Second event" } }

            data: { "delta": { "content": "Third  event" } }
        "#;
        let _mock = wiremock::Mock::given(method("POST"))
            .and(header(CONTENT_TYPE.as_str(), "content/json"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header(CONTENT_TYPE.as_str(), "text/event-stream; charset=utf-8")
                    .set_body_raw(sse_data.as_bytes(), "text/event-stream; charset=utf-8")
                    .set_body_string(sse_data),
            )
            .mount(&mock_server)
            .await;

        let client = NetworkClient::new();
        let settings = AssistantSettings {
            token: "token".to_string(),
            url: mock_server.uri(),
            chat_model: "model".to_string(),
            temperature: 0.5,
            max_tokens: 100,
            max_completion_tokens: 100,
            top_p: 0.5,
            stream: true,
            parallel_tool_calls: false,
            tools: false,
            advertisement: false,
            assistant_role: "".to_string(),
        };

        let messages = vec![TestMessage {
            role: "role".to_string(),
            content: "content".to_string(),
        }];

        let payload = client.prepare_payload(settings.clone(), messages).unwrap();
        let request = client.prepare_request(settings, payload).unwrap();

        let (tx, mut rx) = mpsc::channel(10);

        let result = client
            .execute_response::<Map<String, Value>>(request, Some(tx))
            .await;

        let mut events = vec![];
        while let Some(data) = rx.recv().await {
            events.push(data);
        }

        let binding = result.unwrap();
        let some = binding.get("delta").unwrap().get("content").unwrap();
        assert_eq!(events, vec!["Hello world!", "Second event", "Third  event"]);
        assert_eq!(events.join(""), some.as_str().unwrap().to_string());
    }

    #[tokio::test]
    async fn test_sse_tool_calls_streaming() {
        let mock_server = MockServer::start().await;

        // SSE content for testing tool_calls
        let sse_data = r#"
            data: { "delta": { "content": null, "tool_calls": [{ "index": 0, "id": "tool_1", "type": "function_call", "function": { "name": "fetch_data", "arguments": "{ " }}] } }

            data: { "delta": { "tool_calls": [{ "index": 0, "id": "tool_2", "type": "function_call", "function": { "arguments": "\"param1\": \"value1\"" }}] } }

            data: { "delta": { "tool_calls": [{ "index": 0, "id": "tool_3", "type": "function_call", "function": { "arguments": " }" }}] } }
        "#;

        let _mock = wiremock::Mock::given(method("POST"))
            .and(header(CONTENT_TYPE.as_str(), "content/json"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header(CONTENT_TYPE.as_str(), "text/event-stream; charset=utf-8")
                    .set_body_raw(sse_data.as_bytes(), "text/event-stream; charset=utf-8")
                    .set_body_string(sse_data),
            )
            .mount(&mock_server)
            .await;

        let client = NetworkClient::new();
        let settings = AssistantSettings {
            token: "token".to_string(),
            url: mock_server.uri(),
            chat_model: "model".to_string(),
            temperature: 0.5,
            max_tokens: 100,
            max_completion_tokens: 100,
            top_p: 0.5,
            stream: true,
            parallel_tool_calls: false,
            tools: true,
            advertisement: false,
            assistant_role: "".to_string(),
        };

        let messages = vec![TestMessage {
            role: "role".to_string(),
            content: "content".to_string(),
        }];

        let payload = client.prepare_payload(settings.clone(), messages).unwrap();
        let request = client.prepare_request(settings, payload).unwrap();

        let (tx, mut rx) = mpsc::channel(10);

        let result = client
            .execute_response::<Map<String, Value>>(request, Some(tx))
            .await;

        let mut function_name = vec![];
        while let Some(data) = rx.recv().await {
            function_name.push(data);
        }

        let binding = result.unwrap();
        let tool_calls_array = binding
            .get("delta")
            .unwrap()
            .get("tool_calls")
            .unwrap()
            .as_array()
            .unwrap();

        assert_eq!(function_name.join(""), "fetch_data");

        dbg!(tool_calls_array);
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

        dbg!(tool_calls_array);
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
            .and(header(CONTENT_TYPE.as_str(), "content/json"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header(CONTENT_TYPE.as_str(), "application/json")
                    .set_body_string(non_streaming_data),
            )
            .mount(&mock_server)
            .await;

        let client = NetworkClient::new();
        let settings = AssistantSettings {
            token: "token".to_string(),
            url: mock_server.uri(),
            chat_model: "model".to_string(),
            temperature: 0.5,
            max_tokens: 100,
            max_completion_tokens: 100,
            top_p: 0.5,
            stream: false, // Disable streaming
            parallel_tool_calls: false,
            tools: true,
            advertisement: false,
            assistant_role: "".to_string(),
        };

        let messages = vec![TestMessage {
            role: "role".to_string(),
            content: "content".to_string(),
        }];

        let payload = client.prepare_payload(settings.clone(), messages).unwrap();
        let request = client.prepare_request(settings, payload).unwrap();

        let result: Map<String, Value> = client
            .execute_response::<Map<String, Value>>(request, None)
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
}
