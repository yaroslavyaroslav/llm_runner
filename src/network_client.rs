use futures_util::StreamExt;
use serde::Serialize;
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

        let mut buffer = String::new();

        if let Some(sender) = sender {
            if response.status().is_success() {
                let mut stream = response.bytes_stream();

                while let Some(chunk) = stream.next().await {
                    let chunk = chunk?;

                    let data = String::from_utf8_lossy(&chunk);

                    if let Some(refined_data) = extract_data_field(&data) {
                        if let Err(err) = sender.send(refined_data).await {
                            eprintln!("Failed to send data: {}", err);
                        }
                    }

                    buffer.push_str(&data);
                }

                drop(sender);

                let payload: Result<T, _> = serde_json::from_str(&buffer);
                return match payload {
                    Ok(data) => Ok(data),
                    Err(_) => Err("Failed to deserialize payload".into()),
                };
            } else {
                return Err(format!("Request failed with status: {}", response.status()).into());
            }
        } else {
            // No sender provided, handle as a regular request and deserialize the response body into T
            if response.status().is_success() {
                let payload = response.json::<T>().await?;
                return Ok(payload);
            } else {
                return Err(format!("Request failed with status: {}", response.status()).into());
            }
        }
    }
}

fn extract_data_field(data: &str) -> Option<String> {
    if let Ok(json_value) = serde_json::from_str::<serde_json::Value>(data) {
        if let Some(data_field) = json_value.get("data") {
            return data_field.as_str().map(|s| s.to_string());
        }
    }
    None
}
