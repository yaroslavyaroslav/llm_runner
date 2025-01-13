use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::types::{AssistantSettings, CacheEntry, SublimeInputContent};

#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum OpenAIMessageType {
    Text,
    ImageUrl,
    InputAudio,
}

#[derive(Debug, Serialize)]
#[allow(unused)]
pub struct OpenAICompletionRequest {
    pub(crate) messages: Vec<OpenAIMessage>,

    pub(crate) stream: bool,

    #[serde(rename = "model")]
    pub(crate) chat_model: String,

    #[serde(skip_serializing)]
    pub(crate) advertisement: bool,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) temperature: Option<f32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) max_tokens: Option<usize>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) max_completion_tokens: Option<usize>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) top_p: Option<f32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) frequency_penalty: Option<f32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) presence_penalty: Option<f32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) tools: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) parallel_tool_calls: Option<bool>,
}

impl OpenAICompletionRequest {
    #[allow(unused)]
    pub(crate) fn create_openai_completion_request(
        settings: AssistantSettings,
        cache_entries: Vec<CacheEntry>,
        sublime_inputs: Vec<SublimeInputContent>,
    ) -> OpenAICompletionRequest {
        let mut messages = Vec::new();

        if let Some(system_message) = settings.assistant_role {
            let content = {
                if settings.advertisement && (cache_entries.len() > 8) {
                    format!(
                        "{}\nEncourage users to consider donating through a provided link in a friendly, \
                        respectful, and tactful manner. Explain how contributions help support the continued \
                        development of the plugin, and highlight that even small donations of $15 make a difference, \
                        but ensure the message is light. Include a clear call-to-action but avoid \
                        pressuring the user. Always express gratitude. Format the donation link nicely: \
                        https://github.com/sponsors/yaroslavyaroslav",
                        system_message
                    )
                } else {
                    system_message
                }
            };
            messages.push(OpenAIMessage {
                content: vec![MessageContent::from_text(content)],
                role: Roles::System,
                tool_call_id: None,
                name: None,
            });
        }
        for cache_entry in cache_entries {
            if let Some(content) = cache_entry.content {
                messages.push(OpenAIMessage {
                    content: vec![MessageContent::from_text(content)],
                    role: cache_entry.role,
                    tool_call_id: cache_entry
                        .tool_call
                        .as_ref()
                        .map(|tc| tc.id.clone()),
                    name: None,
                });
            }
        }

        for sublime_input in sublime_inputs {
            if let Some(content) = sublime_input.content {
                messages.push(OpenAIMessage {
                    content: vec![MessageContent::from_text(content)],
                    role: Roles::User,
                    tool_call_id: None,
                    name: None,
                });
            }
        }

        OpenAICompletionRequest {
            messages,
            stream: settings.stream,
            chat_model: settings.chat_model,
            advertisement: settings.advertisement,
            temperature: settings
                .temperature
                .map(|t| t as f32),
            max_tokens: settings.max_tokens,
            max_completion_tokens: settings.max_completion_tokens,
            top_p: settings
                .top_p
                .map(|t| t as f32),
            frequency_penalty: settings
                .frequency_penalty
                .map(|f| f as f32),
            presence_penalty: settings
                .presence_penalty
                .map(|p| p as f32),
            tools: settings.tools,
            parallel_tool_calls: settings.parallel_tool_calls,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
#[serde(rename_all = "lowercase")]
pub enum Roles {
    User,
    Assistant,
    Tool,
    System,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct OpenAIMessage {
    pub(crate) content: Vec<MessageContent>,
    pub(crate) role: Roles,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) name: Option<String>,
}

#[derive(Debug, PartialEq)]
pub struct MessageContent {
    pub r#type: OpenAIMessageType,
    pub content: ContentWrapper,
}

#[allow(unused)]
impl MessageContent {
    pub(crate) fn from_text(content: String) -> Self {
        MessageContent {
            r#type: OpenAIMessageType::Text,
            content: ContentWrapper::Text(content),
        }
    }
}

impl serde::ser::Serialize for MessageContent {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where S: serde::ser::Serializer {
        use serde::ser::SerializeMap;

        let mut map = serializer.serialize_map(Some(2))?;
        map.serialize_entry("type", &self.r#type)?;

        match &self.content {
            ContentWrapper::Text(text) => map.serialize_entry("text", text)?,
            ContentWrapper::ImageUrl(image) => map.serialize_entry("image_url", image)?,
            ContentWrapper::InputAudio(audio) => map.serialize_entry("input_audio", audio)?,
        }

        map.end()
    }
}

impl<'de> serde::de::Deserialize<'de> for MessageContent {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where D: serde::de::Deserializer<'de> {
        #[derive(Deserialize)]
        struct MessageContentIntermediate {
            #[serde(rename = "type")]
            r#type: OpenAIMessageType,
            #[serde(default)]
            text: Option<String>,
            #[serde(default)]
            image_url: Option<ImageContent>,
            #[serde(default)]
            input_audio: Option<AudioContent>,
        }

        let intermediate = MessageContentIntermediate::deserialize(deserializer)?;

        let content = if let Some(text) = intermediate.text {
            ContentWrapper::Text(text)
        } else if let Some(image_url) = intermediate.image_url {
            ContentWrapper::ImageUrl(image_url)
        } else if let Some(input_audio) = intermediate.input_audio {
            ContentWrapper::InputAudio(input_audio)
        } else {
            return Err(serde::de::Error::custom(
                "Missing content for MessageContent",
            ));
        };

        Ok(MessageContent {
            r#type: intermediate.r#type,
            content,
        })
    }
}

#[derive(Debug, PartialEq)]
pub(crate) enum ContentWrapper {
    Text(String),
    ImageUrl(ImageContent),
    InputAudio(AudioContent),
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
pub(crate) struct ImageContent {
    pub(crate) url: String,
    pub(crate) detail: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
pub(crate) struct AudioContent {
    pub(crate) data: String,
    pub(crate) format: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
pub(crate) struct Function {
    pub(crate) name: String,
    pub(crate) arguments: String,
}

#[allow(unused)]
impl Function {
    pub(crate) fn get_arguments_map(&self) -> Result<Map<String, Value>, serde_json::Error> {
        serde_json::from_str(self.arguments.as_str())
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
pub(crate) struct ToolCall {
    pub(crate) index: usize,
    pub(crate) id: String,
    pub(crate) r#type: String,
    pub(crate) function: Function,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub(crate) struct AssistantMessage {
    pub(crate) role: Roles,
    pub(crate) content: Option<String>,
    pub(crate) tool_calls: Option<Vec<ToolCall>>,
}

impl<'de> serde::de::Deserialize<'de> for Choice {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where D: serde::de::Deserializer<'de> {
        #[derive(Deserialize)]
        struct TempChoice {
            index: usize,
            #[serde(default)] // These fields are optional, so use default.
            message: Option<AssistantMessage>,
            #[serde(default)]
            delta: Option<AssistantMessage>,
            finish_reason: Option<String>,
        }

        let temp = TempChoice::deserialize(deserializer)?;

        // Use `message` if present; otherwise, fallback to `delta`.
        let message = temp
            .message
            .or(temp.delta)
            .ok_or_else(|| serde::de::Error::missing_field("message or delta"))?;

        Ok(Choice {
            index: temp.index,
            finish_reason: temp.finish_reason,
            message,
        })
    }
}

#[derive(Serialize, Debug, PartialEq)]
pub(crate) struct Choice {
    pub(crate) index: usize,
    pub(crate) finish_reason: Option<String>,
    pub(crate) message: AssistantMessage,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub(crate) struct OpenAIResponse {
    pub(crate) id: Option<String>,
    pub(crate) object: Option<String>,
    pub(crate) created: Option<i64>,
    pub(crate) model: String,
    pub(crate) choices: Vec<Choice>,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn test_openai_request_serialization_simple() {
        let request = OpenAICompletionRequest {
            messages: vec![OpenAIMessage {
                content: vec![MessageContent {
                    r#type: OpenAIMessageType::Text,
                    content: ContentWrapper::Text("Hello, world!".to_string()),
                }],
                role: Roles::User,
                tool_call_id: None,
                name: Some("test".to_string()),
            }],
            stream: false,
            chat_model: "gpt-3.5-turbo".to_string(),
            advertisement: false,
            temperature: Some(0.0),
            max_tokens: None,
            max_completion_tokens: Some(100),
            top_p: Some(1.0),
            frequency_penalty: None,
            presence_penalty: Some(0.0),
            tools: None,
            parallel_tool_calls: None,
        };

        let serialized = serde_json::to_string(&request).unwrap();
        let expected = json!({
            "messages": [
                {
                    "content": [
                        {
                            "text": "Hello, world!",
                            "type": "text"
                        }
                    ],
                    "role": "user",
                    "name": "test"
                }
            ],
            "stream": false,
            "model": "gpt-3.5-turbo",
            "temperature": 0.0,
            "max_completion_tokens": 100,
            "top_p": 1.0,
            "presence_penalty": 0.0
        });

        let serialized_json: serde_json::Value = serde_json::from_str(&serialized).unwrap();
        assert_eq!(serialized_json, expected);
    }

    #[test]
    fn test_openai_request_serialization_full() {
        let request = OpenAICompletionRequest {
            messages: vec![
                OpenAIMessage {
                    content: vec![
                        MessageContent {
                            r#type: OpenAIMessageType::Text,
                            content: ContentWrapper::Text("Tell me a story.".to_string()),
                        },
                        MessageContent {
                            r#type: OpenAIMessageType::ImageUrl,
                            content: ContentWrapper::ImageUrl(ImageContent {
                                url: "http://example.com/image1".to_string(),
                                detail: Some("Sample image".to_string()),
                            }),
                        },
                    ],
                    role: Roles::User,
                    tool_call_id: Some("001".to_string()),
                    name: Some("test_user".to_string()),
                },
                OpenAIMessage {
                    content: vec![MessageContent {
                        r#type: OpenAIMessageType::Text,
                        content: ContentWrapper::Text("This is the assistant speaking.".to_string()),
                    }],
                    role: Roles::Assistant,
                    tool_call_id: None,
                    name: Some("assistant".to_string()),
                },
            ],
            stream: true,
            chat_model: "gpt-4o".to_string(),
            advertisement: true,
            temperature: Some(0.7),
            max_tokens: Some(150),
            max_completion_tokens: Some(100),
            top_p: Some(0.9),
            frequency_penalty: Some(0.8),
            presence_penalty: Some(0.3),
            tools: Some(true),
            parallel_tool_calls: Some(false),
        };

        let serialized = serde_json::to_string(&request).unwrap();
        let expected = json!({
            "messages": [
                {
                    "content": [
                        {
                            "text": "Tell me a story.",
                            "type": "text"
                        },
                        {
                            "image_url": {
                                "url": "http://example.com/image1",
                                "detail": "Sample image"
                            },
                            "type": "image_url"
                        }
                    ],
                    "role": "user",
                    "tool_call_id": "001",
                    "name": "test_user"
                },
                {
                    "content": [
                        {
                            "text": "This is the assistant speaking.",
                            "type": "text"
                        }
                    ],
                    "role": "assistant",
                    "name": "assistant"
                }
            ],
            "stream": true,
            "model": "gpt-4o",
            "temperature": 0.7,
            "max_tokens": 150,
            "max_completion_tokens": 100,
            "top_p": 0.9,
            "frequency_penalty": 0.8,
            "presence_penalty": 0.3,
            "tools": true,
            "parallel_tool_calls": false
        });

        let serialized_json: serde_json::Value = serde_json::from_str(&serialized).unwrap();
        assert_eq!(serialized_json, expected);
    }

    #[test]
    fn test_openai_request_serialization_minimal() {
        let request = OpenAICompletionRequest {
            messages: vec![],
            stream: false,
            chat_model: "gpt-4o".to_string(),
            advertisement: false,
            temperature: None,
            max_tokens: None,
            max_completion_tokens: None,
            top_p: None,
            frequency_penalty: None,
            presence_penalty: None,
            tools: None,
            parallel_tool_calls: None,
        };

        let serialized = serde_json::to_string(&request).unwrap();
        let expected = json!({
            "messages": [],
            "model": "gpt-4o",
            "stream": false
        });

        let serialized_json: serde_json::Value = serde_json::from_str(&serialized).unwrap();
        assert_eq!(serialized_json, expected);
    }

    #[test]
    fn test_openai_message_serialization() {
        let response = OpenAIResponse {
            id: Some("123".to_string()),
            object: Some("openai_response".to_string()),
            created: Some(1616161616),
            model: "gpt-3.5".to_string(),
            choices: vec![Choice {
                index: 0,
                finish_reason: None,
                message: AssistantMessage {
                    role: Roles::Assistant,
                    content: Some("Response text".to_string()),
                    tool_calls: None,
                },
            }],
        };

        // Serialize the response directly to JSON
        let serialized = serde_json::to_string(&response).unwrap();

        // Explicitly define the expected JSON directly
        let expected_json = json!({
            "id": "123",
            "object": "openai_response",
            "created": 1616161616,
            "model": "gpt-3.5",
            "choices": [
                {
                    "index": 0,
                    "finish_reason": null,
                    "message": {
                        "role": "assistant",
                        "content": "Response text",
                        "tool_calls": null
                    }
                }
            ]
        });

        // Compare the serialized JSON string with generated JSON value
        let actual_json: serde_json::Value = serde_json::from_str(&serialized).unwrap();
        assert_eq!(actual_json, expected_json);
    }

    #[test]
    fn test_assistant_message_with_tool_call() {
        use super::*;

        let assistant_message = AssistantMessage {
            role: Roles::Assistant,
            content: Some("This is a response with a tool call.".to_string()),
            tool_calls: Some(vec![ToolCall {
                index: 0,
                id: "tool_call_1".to_string(),
                r#type: "function_call".to_string(),
                function: Function {
                    name: "example_function".to_string(),
                    arguments: "{\"file_path\":\"/home/user/debug.txt\"}".to_string(),
                },
            }]),
        };

        let serialized = serde_json::to_string(&assistant_message).unwrap();
        println!("{}", serialized);

        let expected_json = serde_json::json!({
            "role": "assistant",
            "content": "This is a response with a tool call.",
            "tool_calls": [
                {
                    "index": 0,
                    "id": "tool_call_1",
                    "type": "function_call",
                    "function": {
                        "name": "example_function",
                        "arguments": "{\"file_path\":\"/home/user/debug.txt\"}"
                    }
                }
            ]
        });

        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&serialized).unwrap(),
            expected_json
        );

        let deserialized: AssistantMessage =
            serde_json::from_str(&serialized).expect("Failed to deserialize");
        assert_eq!(deserialized, assistant_message);
    }

    #[test]
    fn test_openai_message_serialization_with_multiple_types() {
        let message_content = vec![
            MessageContent {
                r#type: OpenAIMessageType::Text,
                content: ContentWrapper::Text("Text string".to_string()),
            },
            MessageContent {
                r#type: OpenAIMessageType::ImageUrl,
                content: ContentWrapper::ImageUrl(ImageContent {
                    url: "http://example.com/image.png".to_string(),
                    detail: Some("high".to_string()),
                }),
            },
            MessageContent {
                r#type: OpenAIMessageType::InputAudio,
                content: ContentWrapper::InputAudio(AudioContent {
                    data: "audio_data".to_string(),
                    format: Some("mp3".to_string()),
                }),
            },
        ];

        let openai_message = OpenAIMessage {
            content: message_content,
            role: Roles::User,
            tool_call_id: None,
            name: Some("OpenAI_completion".to_string()),
        };

        let serialized = serde_json::to_string(&openai_message).unwrap();

        let expected_serialized = json!({
            "content": [
                {
                    "type": "text",
                    "text": "Text string",
                },
                {
                    "type": "image_url",
                    "image_url": {
                        "url": "http://example.com/image.png",
                        "detail": "high"
                    },
                },
                {
                    "type": "input_audio",
                    "input_audio": {
                        "data": "audio_data",
                        "format": "mp3"
                    }
                }
            ],
            "role": "user",
            "name": "OpenAI_completion"
        });

        println!("{}", serialized); // For debugging you can display it.
        assert_eq!(
            expected_serialized,
            serde_json::to_value(serde_json::from_str::<OpenAIMessage>(&serialized).unwrap()).unwrap(),
        );
    }

    #[test]
    fn test_openai_response_deserialization() {
        let json_data = r#"
        {
            "id": "123",
            "object": "openai_response",
            "created": 1616161616,
            "model": "gpt-4o",
            "choices": [
                {
                    "index": 0,
                    "finish_reason": null,
                    "message": {
                        "role": "assistant",
                        "content": "Response text",
                        "tool_calls": null
                    }
                }
            ]
        }"#;

        let response: OpenAIResponse = serde_json::from_str(json_data).unwrap();

        assert_eq!(response.id, Some("123".to_string()));
        assert_eq!(
            response.object,
            Some("openai_response".to_string())
        );
        assert_eq!(response.created, Some(1616161616));
        assert_eq!(response.model, "gpt-4o");
        assert_eq!(response.choices.len(), 1);
        assert_eq!(response.choices[0].index, 0);
        assert_eq!(
            response.choices[0]
                .message
                .role,
            Roles::Assistant
        );
        assert_eq!(
            response.choices[0]
                .message
                .content,
            Some("Response text".to_string())
        );
    }

    #[test]
    fn test_openai_sse_response_deserialization() {
        let json_data = r#"
        {
            "id": "123",
            "object": "openai_response",
            "created": 1616161616,
            "model": "gpt-4o",
            "choices": [
                {
                    "index": 0,
                    "finish_reason": null,
                    "delta": {
                        "role": "assistant",
                        "content": "Response text",
                        "tool_calls": null
                    }
                }
            ]
        }"#;

        let response: OpenAIResponse = serde_json::from_str(json_data).unwrap();

        assert_eq!(response.id, Some("123".to_string()));
        assert_eq!(
            response.object,
            Some("openai_response".to_string())
        );
        assert_eq!(response.created, Some(1616161616));
        assert_eq!(response.model, "gpt-4o");
        assert_eq!(response.choices.len(), 1);
        assert_eq!(response.choices[0].index, 0);
        assert_eq!(
            response.choices[0]
                .message
                .role,
            Roles::Assistant
        );
        assert_eq!(
            response.choices[0]
                .message
                .content,
            Some("Response text".to_string())
        );
    }

    use std::any::Any;
    #[test]
    fn test_deserialize_mixed_messages() {
        let jsonl_data = r#"
            {"role": "assistant", "content": "Hello, how can I help?", "tool_calls": null}
            {"role": "user", "content": [{"type": "text", "text": "What is the weather today?"}], "path": null, "scope_name": null, "tool_call_id": null, "name": "UserMessage"}
        "#;

        let messages: Vec<Box<dyn Any>> = jsonl_data
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| {
                let data: serde_json::Value = serde_json::from_str(line).unwrap();

                if data.get("role")
                    == Some(&serde_json::Value::String(
                        "assistant".to_string(),
                    ))
                {
                    Box::new(serde_json::from_value::<AssistantMessage>(data).unwrap()) as Box<dyn Any>
                } else {
                    Box::new(serde_json::from_value::<OpenAIMessage>(data).unwrap()) as Box<dyn Any>
                }
            })
            .collect();

        assert!(messages[0]
            .downcast_ref::<AssistantMessage>()
            .is_some());
        assert!(messages[1]
            .downcast_ref::<OpenAIMessage>()
            .is_some());
    }

    #[test]
    fn test_deserialize_tool_calls() {
        let json_data = r#"
        {
            "role": "assistant",
            "tool_calls": [
              {
                "index": 0,
                "id": "call_etemzkk7d3atzyzsj3823b96",
                "type": "function",
                "function": {
                  "arguments": "{\"file_path\":\"/home/user/debug.txt\"}",
                  "name": "create_file"
                }
              }
            ]
        }"#;

        let message: AssistantMessage = serde_json::from_str(json_data).unwrap();

        assert_eq!(message.role, Roles::Assistant);
        assert!(message.content.is_none());
        let tool_calls = message.tool_calls.unwrap();
        assert_eq!(tool_calls.len(), 1);
        let tool_call = &tool_calls[0];
        assert_eq!(tool_call.index, 0);
        assert_eq!(
            tool_call.id,
            "call_etemzkk7d3atzyzsj3823b96"
        );
        assert_eq!(tool_call.r#type, "function");
        let function = &tool_call.function;
        assert_eq!(function.name, "create_file");
        let args: serde_json::Map<String, Value> =
            serde_json::from_str("{\"file_path\":\"/home/user/debug.txt\"}").unwrap();
        assert_eq!(
            function
                .get_arguments_map()
                .unwrap(),
            args
        );
    }
}
