use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum OpenAIMessageType {
    Text,
    ImageUrl,
    InputAudio,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "snake_case")]
pub struct AssistantSettings {
    pub token: String,
    pub url: String,
    pub chat_model: String,
    pub temperature: f64,
    pub max_tokens: i32,
    pub max_completion_tokens: i32,
    pub top_p: f64,
    pub stream: bool,
    pub parallel_tool_calls: bool,
    pub tools: bool,
    pub advertisement: bool,
    pub assistant_role: String,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Roles {
    User,
    Assistant,
    Tool,
    System,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct Content {
    pub scope_name: Option<String>,
    pub path: Option<String>,
    pub content: String,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct OpenAIMessage {
    pub content: Vec<MessageContent>,
    pub role: Roles,
    pub path: Option<String>,
    pub scope_name: Option<String>,
    pub tool_call_id: Option<String>,
    pub name: Option<String>,
}

impl OpenAIMessage {
    pub fn from_content(content: Vec<Content>) -> Self {
        let contents = content
            .iter()
            .map(|item| MessageContent {
                r#type: OpenAIMessageType::Text,
                content: ContentWrapper::Text(item.content.clone()),
            })
            .collect();

        OpenAIMessage {
            content: contents,
            role: Roles::User,
            path: content.get(0).and_then(|c| c.path.clone()),
            scope_name: content.get(0).and_then(|c| c.scope_name.clone()),
            tool_call_id: None,
            name: Some("OpenAI_completion".to_string()),
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct MessageContent {
    pub r#type: OpenAIMessageType,
    pub content: ContentWrapper,
}

#[derive(Debug, PartialEq)]
pub enum ContentWrapper {
    Text(String),
    ImageUrl(ImageContent),
    InputAudio(AudioContent),
}

impl serde::ser::Serialize for MessageContent {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
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
    where
        D: serde::de::Deserializer<'de>,
    {
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

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
pub struct ImageContent {
    pub url: String,
    pub detail: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
pub struct AudioContent {
    pub data: String,
    pub format: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct Function {
    pub name: String,
    pub arguments: HashMap<String, serde_json::Value>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct ToolCall {
    pub index: usize,
    pub id: String,
    pub r#type: String,
    pub function: Function,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct AssistantMessage {
    pub role: Roles,
    pub content: Option<String>,
    pub tool_calls: Option<Vec<ToolCall>>,
}

impl<'de> serde::de::Deserialize<'de> for Choice {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
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
pub struct Choice {
    pub index: usize,
    pub finish_reason: Option<String>,
    pub message: AssistantMessage,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct OpenAIResponse {
    pub id: Option<String>,
    pub object: Option<String>,
    pub created: Option<i64>,
    pub model: String,
    pub choices: Vec<Choice>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
                    arguments: HashMap::from([
                        ("arg1".to_string(), serde_json::json!("value1")),
                        ("arg2".to_string(), serde_json::json!("value2")),
                    ]),
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
                        "arguments": {
                            "arg1": "value1",
                            "arg2": "value2"
                        }
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
            path: None,
            scope_name: None,
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
            "path": null,
            "scope_name": null,
            "tool_call_id": null,
            "name": "OpenAI_completion"
        });

        println!("{}", serialized); // For debugging you can display it.
        assert_eq!(
            serde_json::to_value(serde_json::from_str::<OpenAIMessage>(&serialized).unwrap())
                .unwrap(),
            expected_serialized
        );
    }

    #[test]
    fn test_openai_response_deserialization() {
        let json_data = r#"
        {
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
        }"#;

        let response: OpenAIResponse = serde_json::from_str(json_data).unwrap();

        assert_eq!(response.id, Some("123".to_string()));
        assert_eq!(response.object, Some("openai_response".to_string()));
        assert_eq!(response.created, Some(1616161616));
        assert_eq!(response.model, "gpt-3.5");
        assert_eq!(response.choices.len(), 1);
        assert_eq!(response.choices[0].index, 0);
        assert_eq!(response.choices[0].message.role, Roles::Assistant);
        assert_eq!(
            response.choices[0].message.content,
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

                if data.get("role") == Some(&serde_json::Value::String("assistant".to_string())) {
                    Box::new(serde_json::from_value::<AssistantMessage>(data).unwrap())
                        as Box<dyn Any>
                } else {
                    Box::new(serde_json::from_value::<OpenAIMessage>(data).unwrap()) as Box<dyn Any>
                }
            })
            .collect();

        assert!(messages[0].downcast_ref::<AssistantMessage>().is_some());
        assert!(messages[1].downcast_ref::<OpenAIMessage>().is_some());
    }
}
