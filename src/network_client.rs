use futures_util::StreamExt;
use serde::Serialize;
use serde_json::{Map, Value};
use std::error::Error;

use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use reqwest::{Client, Request};
use serde::de::DeserializeOwned;
use tokio::sync::mpsc;

use crate::types::AssistantSettings;

// Define the errors
#[derive(Debug)]
pub(crate) enum OpenAIErrors {
    ContextLengthExceededException,
    UnknownException,
    ReqwestError(reqwest::Error),
    InvalidHeaderError(String),
    JsonError(serde_json::Error),
}
// Define the network client
pub(crate) struct NetworkClient {
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

    // Prepare and execute the request
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
                .map_err(|e| OpenAIErrors::JsonError(e))?
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
                    .map_err(|e| OpenAIErrors::JsonError(e))?,
            );
            internal_messages
        };

        serde_json::to_string(&internal_messages).map_err(|e| OpenAIErrors::JsonError(e))
    }

    pub(crate) fn prepare_request(
        &self,
        settings: AssistantSettings,
        json_payload: String,
    ) -> Result<Request, OpenAIErrors> {
        let url = format!("{}", settings.url);
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
            .map_err(|e| OpenAIErrors::ReqwestError(e))
    }

    pub async fn execute_response<T>(
        &self,
        request: Request,
        sender: Option<mpsc::Sender<String>>, // Sender for "data" field updates
    ) -> Result<T, Box<dyn Error>>
    where
        T: DeserializeOwned, // Ensures T can be deserialized from JSON
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

                return Ok(serde_json::from_value::<T>(composable_response)?);
            } else {
                return Err(format!("Request failed with status: {}", response.status()).into());
            }
        } else {
            if response.status().is_success() {
                let payload = response.json::<T>().await?;
                return Ok(payload);
            } else {
                return Err(format!("Request failed with status: {}", response.status()).into());
            }
        }
    }
}

fn merge_json(base: &mut Value, addition: &Value) {
    match (base, addition) {
        (Value::Object(base_map), Value::Object(addition_map)) => {
            for (key, value) in addition_map {
                if key == "content" && base_map.contains_key(key) {
                    if let Some(Value::String(existing_value)) = base_map.get_mut(key) {
                        if let Value::String(addition_value) = value {
                            existing_value.push_str(addition_value);
                            continue; // Skip default merge logic for "content"
                        }
                    }
                }
                merge_json(base_map.entry(key).or_insert(Value::Null), value);
            }
        }
        (base, addition) => {
            *base = addition.clone();
        }
    }
}

fn obtain_delta(map: &Map<String, Value>) -> Option<String> {
    if let Some(delta) = map.get("delta") {
        if let Some(content) = delta.get("content") {
            return content.as_str().map(|s| s.to_string());
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
}
