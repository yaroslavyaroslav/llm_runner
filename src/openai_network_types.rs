use anyhow::Result;
use pyo3::pyclass;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use strum_macros::{Display, EnumString};

use crate::{
    tools_definition::FUNCTIONS,
    types::{ApiType, AssistantSettings, CacheEntry, InputKind, ReasonEffort, SublimeInputContent},
};

#[derive(Debug)]
pub enum OpenAIRequestMessage {
    OpenAIMessage(OpenAIMessage),
    OpenAIPlainTextMessage(OpenAIPlainTextMessage),
}

impl OpenAIRequestMessage {
    pub(crate) fn weight(&self) -> u8 {
        match self {
            OpenAIRequestMessage::OpenAIMessage(msg) => msg.kind.weight(),
            OpenAIRequestMessage::OpenAIPlainTextMessage(plain) => plain.kind.weight(),
        }
    }
}

impl serde::ser::Serialize for OpenAIRequestMessage {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where S: serde::ser::Serializer {
        match self {
            OpenAIRequestMessage::OpenAIMessage(message) => message.serialize(serializer),
            OpenAIRequestMessage::OpenAIPlainTextMessage(message) => message.serialize(serializer),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) enum ErrorResponse {
    OpenAI(OpenAIErrorContainer),
    Other(OtherErrorContainer),
    Message(String),
}

impl ErrorResponse {
    pub(crate) fn message(&self) -> String {
        match self {
            ErrorResponse::OpenAI(err) => err.message(),
            ErrorResponse::Other(err) => err.message(),
            ErrorResponse::Message(msg) => msg.clone(),
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub(crate) struct OpenAIErrorContainer {
    pub(crate) error: OpenAIError,
}

impl OpenAIErrorContainer {
    fn message(&self) -> String { self.error.message.clone() }
}

#[derive(Debug, Deserialize, Clone)]
pub(crate) struct OpenAIError {
    pub(crate) message: String,
}

#[derive(Debug, Deserialize, Clone)]
pub(crate) struct OtherErrorContainer {
    error: String,
}

impl OtherErrorContainer {
    fn message(&self) -> String { self.error.clone() }
}

#[derive(Debug, Serialize)]
#[allow(unused)]
pub struct OpenAICompletionRequest {
    pub(crate) messages: Vec<OpenAIRequestMessage>,

    pub(crate) stream: bool,

    #[serde(rename = "model")]
    pub(crate) chat_model: String,

    #[serde(skip_serializing)]
    pub(crate) advertisement: bool,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) temperature: Option<f64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) max_tokens: Option<usize>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) max_completion_tokens: Option<usize>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) top_p: Option<f64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) frequency_penalty: Option<f64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) presence_penalty: Option<f64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) reasoning_effort: Option<ReasonEffort>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) parallel_tool_calls: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) tools: Option<Vec<Tool>>,
}

fn convert_message<T>(item: T, api_type: ApiType) -> OpenAIRequestMessage
where
    OpenAIMessage: From<T>,
    OpenAIPlainTextMessage: From<T>, {
    match api_type {
        ApiType::OpenAi => OpenAIRequestMessage::OpenAIMessage(OpenAIMessage::from(item)),
        ApiType::PlainText => {
            OpenAIRequestMessage::OpenAIPlainTextMessage(OpenAIPlainTextMessage::from(item))
        }
        ApiType::Antropic => todo!(),
    }
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

            if dbg!(settings.api_type) == ApiType::OpenAi {
                messages.push(OpenAIRequestMessage::OpenAIMessage(
                    OpenAIMessage::from_system(content),
                ))
            } else if settings.api_type == ApiType::PlainText {
                messages.push(
                    OpenAIRequestMessage::OpenAIPlainTextMessage(OpenAIPlainTextMessage::from_system(
                        content,
                    )),
                )
            }
        }

        messages.extend(
            cache_entries
                .into_iter()
                .map(|c| convert_message(c, settings.api_type)),
        );
        messages.extend(
            sublime_inputs
                .into_iter()
                .map(|c| convert_message(c, settings.api_type)),
        );

        messages.sort_by_key(|m| m.weight());

        OpenAICompletionRequest {
            messages,
            stream: settings.stream,
            chat_model: settings.chat_model,
            advertisement: settings.advertisement,
            temperature: settings.temperature,
            max_tokens: settings.max_tokens,
            max_completion_tokens: settings.max_completion_tokens,
            reasoning_effort: settings.reasoning_effort,
            top_p: settings.top_p,
            frequency_penalty: settings.frequency_penalty,
            presence_penalty: settings.presence_penalty,
            tools: if settings
                .tools
                .unwrap_or(false)
            {
                Some(
                    FUNCTIONS
                        .iter()
                        .map(|tool| tool.as_ref().clone())
                        .collect::<Vec<Tool>>(),
                )
            } else {
                None
            },
            parallel_tool_calls: settings.parallel_tool_calls,
        }
    }
}

#[derive(Serialize, Debug, PartialEq, Eq)]
pub(crate) enum MessageKind {
    SystemMessage,

    SheetContent,
    CacheEntry,
    OutputPaneContent,
    ViewSelection,

    FunctionResult,
    UserCommand,
}

impl From<InputKind> for MessageKind {
    fn from(value: InputKind) -> Self {
        match value {
            InputKind::Command => Self::UserCommand,
            InputKind::ViewSelection => Self::ViewSelection,
            InputKind::BuildOutputPanel | InputKind::LspOutputPanel | InputKind::Terminus => {
                Self::OutputPaneContent
            }
            InputKind::Sheet => Self::SheetContent,
            InputKind::FunctionResult => Self::FunctionResult,
            InputKind::AssistantResponse => Self::CacheEntry,
        }
    }
}

impl MessageKind {
    /// Return a weight to determine the order.
    pub(crate) fn weight(&self) -> u8 {
        match self {
            Self::SystemMessage => 0,
            Self::SheetContent => 1,
            Self::CacheEntry => 2,
            Self::OutputPaneContent => 3,
            Self::ViewSelection => 4,
            Self::UserCommand | Self::FunctionResult => 5,
        }
    }
}

#[derive(Serialize, Debug, PartialEq)]
pub struct OpenAIMessage {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) content: Option<Vec<MessageContent>>,

    pub(crate) role: Roles,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) tool_call_id: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) tool_calls: Option<Vec<ToolCall>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) name: Option<String>,

    #[serde(skip_serializing)]
    pub(crate) kind: MessageKind,
}

impl OpenAIMessage {
    pub(crate) fn from_system(value: String) -> Self {
        OpenAIMessage {
            content: vec![MessageContent::from_text(value)].into(),
            role: Roles::Developer,
            tool_call_id: None,
            name: None,
            tool_calls: None,
            kind: MessageKind::SystemMessage,
        }
    }
}

impl From<CacheEntry> for OpenAIMessage {
    fn from(value: CacheEntry) -> Self {
        OpenAIMessage {
            content: value
                .content
                .map(|c| vec![MessageContent::from_text(c)]),
            role: value.role,
            tool_call_id: value.tool_call_id,
            name: None,
            tool_calls: value.tool_calls,
            kind: MessageKind::CacheEntry,
        }
    }
}

impl From<SublimeInputContent> for OpenAIMessage {
    fn from(value: SublimeInputContent) -> Self {
        Self {
            content: value
                .content
                .map(|c| vec![MessageContent::from_text(c)]),
            role: if value.tool_id.is_some() { Roles::Tool } else { Roles::User },
            tool_call_id: value.tool_id,
            name: None,
            tool_calls: None,
            kind: MessageKind::from(value.input_kind),
        }
    }
}

#[derive(Serialize, Debug, PartialEq)]
pub struct OpenAIPlainTextMessage {
    pub(crate) content: String,

    pub(crate) role: Roles,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) tool_call_id: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) tool_calls: Option<Vec<ToolCall>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) name: Option<String>,

    #[serde(skip_serializing)]
    pub(crate) kind: MessageKind,
}

impl OpenAIPlainTextMessage {
    pub(crate) fn from_system(value: String) -> Self {
        Self {
            content: value,
            role: Roles::System,
            tool_call_id: None,
            name: None,
            tool_calls: None,
            kind: MessageKind::SystemMessage,
        }
    }
}

impl From<CacheEntry> for OpenAIPlainTextMessage {
    fn from(value: CacheEntry) -> Self {
        Self {
            content: value.combined_content(),
            role: value.role,
            tool_call_id: value.tool_call_id,
            name: None,
            tool_calls: value.tool_calls,
            kind: MessageKind::CacheEntry,
        }
    }
}

impl From<SublimeInputContent> for OpenAIPlainTextMessage {
    fn from(value: SublimeInputContent) -> Self {
        Self {
            content: value.combined_content(),
            role: if value.tool_id.is_some() { Roles::Tool } else { Roles::User },
            tool_call_id: value.tool_id,
            name: None,
            tool_calls: None,
            kind: MessageKind::from(value.input_kind),
        }
    }
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

#[pyclass(eq, eq_int)]
#[derive(EnumString, Display, Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Roles {
    User,
    Assistant,
    Tool,
    System,
    Developer,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum OpenAIMessageType {
    Text,
    ImageUrl,
    InputAudio,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
#[serde(rename_all = "snake_case")]
pub struct Tool {
    pub(crate) r#type: String,
    pub(crate) function: Option<FunctionToCall>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
#[serde(rename_all = "snake_case")]
pub struct FunctionToCall {
    pub(crate) name: String,
    pub(crate) description: Option<String>,
    pub(crate) parameters: Option<Map<String, Value>>,
    pub(crate) strict: Option<bool>,
}

// --- Response ---

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
pub(crate) struct OpenAIResponse {
    pub(crate) id: Option<String>,
    pub(crate) object: Option<String>,
    pub(crate) created: Option<i64>,
    pub(crate) model: String,
    pub(crate) choices: Vec<Choice>,
}

#[derive(Serialize, Debug, PartialEq, Clone)]
pub(crate) struct Choice {
    pub(crate) index: usize,
    pub(crate) finish_reason: Option<String>,
    pub(crate) message: AssistantMessage,
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

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
pub(crate) struct AssistantMessage {
    pub(crate) role: Roles,
    pub(crate) content: Option<String>,
    pub(crate) tool_calls: Option<Vec<ToolCall>>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
pub(crate) struct ToolCall {
    // pub(crate) index: usize,
    pub(crate) id: String,
    pub(crate) r#type: String,
    pub(crate) function: Function,
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

#[cfg(test)]
mod tests {

    use serde_json::json;

    use super::*;

    // Helper to create a dummy cache entry.
    fn dummy_cache_entry() -> CacheEntry {
        CacheEntry {
            content: Some("Cache entry".to_string()),
            role: Roles::Assistant, // this conversion produces MessageKind::CacheEntry
            tool_call_id: None,
            tool_calls: None,
            thinking: None,
            path: None,
            scope: None,
        }
    }

    // Create a SublimeInputContent for a given kind and content.
    fn dummy_sublime_input(content: &str, kind: InputKind) -> SublimeInputContent {
        SublimeInputContent {
            content: Some(content.to_string()),
            tool_id: None,
            input_kind: kind,
            path: None,
            scope: None,
        }
    }

    // Utility to extract the weight of a message.
    fn message_weight(message: &OpenAIRequestMessage) -> u8 {
        match message {
            OpenAIRequestMessage::OpenAIMessage(msg) => msg.kind.weight(),
            OpenAIRequestMessage::OpenAIPlainTextMessage(msg) => msg.kind.weight(),
        }
    }

    fn dummy_settings(api_type: ApiType) -> AssistantSettings {
        let mut assistant = AssistantSettings::default();
        assistant.assistant_role = Some("System role".to_string());
        assistant.api_type = api_type;
        assistant.stream = false;
        assistant.chat_model = "dummy-model".to_string();
        assistant.advertisement = false;
        assistant.temperature = None;
        assistant.max_tokens = None;
        assistant.max_completion_tokens = None;
        assistant.top_p = None;
        assistant.frequency_penalty = None;
        assistant.presence_penalty = None;
        assistant.reasoning_effort = None;
        assistant.tools = Some(false);
        assistant.parallel_tool_calls = None;
        assistant
    }

    fn dummy_cache_entry_with_role(role: Roles, content: &str) -> CacheEntry {
        CacheEntry {
            content: Some(content.to_string()),
            role,
            tool_call_id: None,
            tool_calls: None,
            thinking: None,
            path: None,
            scope: None,
        }
    }

    #[test]
    fn test_is_sync() {
        fn is_sync<T: Sync>() {}

        is_sync::<Function>();
        is_sync::<ToolCall>();
        is_sync::<AssistantMessage>();
        is_sync::<Choice>();
        is_sync::<OpenAIResponse>();
        is_sync::<FunctionToCall>();
        is_sync::<Tool>();
        is_sync::<OpenAIMessageType>();
        is_sync::<Roles>();
        is_sync::<AudioContent>();
        is_sync::<ImageContent>();
        is_sync::<ContentWrapper>();
        is_sync::<MessageContent>();
        is_sync::<OpenAIMessage>();
        is_sync::<OpenAICompletionRequest>();
        is_sync::<OpenAICompletionRequest>();
    }

    #[test]
    fn test_is_send() {
        fn is_send<T: Send>() {}

        is_send::<Function>();
        is_send::<ToolCall>();
        is_send::<AssistantMessage>();
        is_send::<Choice>();
        is_send::<OpenAIResponse>();
        is_send::<FunctionToCall>();
        is_send::<Tool>();
        is_send::<OpenAIMessageType>();
        is_send::<Roles>();
        is_send::<AudioContent>();
        is_send::<ImageContent>();
        is_send::<ContentWrapper>();
        is_send::<MessageContent>();
        is_send::<OpenAIMessage>();
        is_send::<OpenAICompletionRequest>();
        is_send::<OpenAICompletionRequest>();
    }

    #[test]
    fn test_openai_request_serialization_simple() {
        let request = OpenAICompletionRequest {
            messages: vec![OpenAIRequestMessage::OpenAIMessage(
                OpenAIMessage {
                    content: vec![MessageContent {
                        r#type: OpenAIMessageType::Text,
                        content: ContentWrapper::Text("Hello, world!".to_string()),
                    }]
                    .into(),
                    role: Roles::User,
                    tool_call_id: None,
                    name: Some("test".to_string()),
                    tool_calls: None,
                    kind: MessageKind::UserCommand,
                },
            )],
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
            reasoning_effort: None,
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
                OpenAIRequestMessage::OpenAIMessage(OpenAIMessage {
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
                    ]
                    .into(),
                    role: Roles::User,
                    tool_call_id: Some("001".to_string()),
                    name: Some("test_user".to_string()),
                    tool_calls: None,
                    kind: MessageKind::UserCommand,
                }),
                OpenAIRequestMessage::OpenAIMessage(OpenAIMessage {
                    content: vec![MessageContent {
                        r#type: OpenAIMessageType::Text,
                        content: ContentWrapper::Text("This is the assistant speaking.".to_string()),
                    }]
                    .into(),
                    role: Roles::Assistant,
                    tool_call_id: None,
                    name: Some("assistant".to_string()),
                    tool_calls: None,
                    kind: MessageKind::UserCommand,
                }),
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
            tools: Some(vec![Tool {
                r#type: "function".to_string(),
                function: Some(FunctionToCall {
                    name: "create_file".to_string(),
                    description: Some(
                        "Create a new file with the specified content at the given path.".to_string(),
                    ),
                    parameters: json!({
                        "properties": {
                            "file_path": {
                                "type": "string",
                                "description": "The path where the file will be created."
                            }
                        },
                        "type": "object",
                        "required": ["file_path"],
                        "additionalProperties": false
                    })
                    .as_object()
                    .cloned(),
                    strict: Some(true),
                }),
            }]),

            parallel_tool_calls: Some(false),
            reasoning_effort: None,
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
            "tools": [json!({
                "type": "function",
                "function": {
                    "name": "create_file",
                    "parameters": {
                        "properties": {
                            "file_path": {
                                "type": "string",
                                "description": "The path where the file will be created."
                            }
                        },
                        "type": "object",
                        "required": ["file_path"],
                        "additionalProperties": false
                    },
                    "description": "Create a new file with the specified content at the given path.",
                    "strict": true
                }
            })],
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
            reasoning_effort: None,
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
    fn test_openai_message_serialization_with_multiple_types_no_deserialization() {
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
            // Note: `kind` is skipped during serialization.
            content: Some(message_content),
            role: Roles::User,
            tool_call_id: None,
            name: Some("OpenAI_completion".to_string()),
            tool_calls: None,
            kind: MessageKind::UserCommand,
        };

        let serialized = serde_json::to_string(&openai_message).unwrap();

        let expected_serialized = r#"{"content":[{"type":"text","text":"Text string"},{"type":"image_url","image_url":{"url":"http://example.com/image.png","detail":"high"}},{"type":"input_audio","input_audio":{"data":"audio_data","format":"mp3"}}],"role":"user","name":"OpenAI_completion"}"#;

        println!("{}", serialized); // For debugging purposes.
        assert_eq!(serialized, expected_serialized);
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

    #[test]
    fn test_parse_mixed_messages_without_deserialization() {
        // Each JSON line, trimmed.
        let jsonl_data = r#"
    {"role": "assistant", "content": "Hello, how can I help?", "tool_calls": null}
    {"role": "user", "content": [{"type": "text", "text": "What is the weather today?"}], "path": null, "scope_name": null, "tool_call_id": null, "name": "UserMessage"}
        "#;

        // A helper that “parses” a JSON-line by simply checking for the assistant role.
        // In a real scenario, this would be your custom parser.
        fn parse_message_line(line: &str) -> Box<dyn std::any::Any> {
            if line.contains("\"role\": \"assistant\"") {
                // Return an AssistantMessage (constructed manually with the expected values)
                Box::new(AssistantMessage {
                    role: Roles::Assistant,
                    content: Some("Hello, how can I help?".to_string()),
                    tool_calls: None,
                }) as Box<dyn std::any::Any>
            } else {
                // Otherwise, return an OpenAIMessage
                Box::new(OpenAIMessage {
                    role: Roles::User,
                    content: Some(vec![MessageContent {
                        r#type: OpenAIMessageType::Text,
                        content: ContentWrapper::Text("What is the weather today?".to_string()),
                    }]),
                    tool_call_id: None,
                    name: Some("UserMessage".to_string()),
                    tool_calls: None,
                    kind: MessageKind::UserCommand,
                }) as Box<dyn std::any::Any>
            }
        }

        let messages: Vec<Box<dyn std::any::Any>> = jsonl_data
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| parse_message_line(line))
            .collect();

        // Now check that the first message is an AssistantMessage and the second is an OpenAIMessage.
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

    // Test for the OpenAi branch when the last sublime input yields a UserCommand.
    #[test]
    fn test_create_completion_request_openaip_user_command_last() {
        let settings = dummy_settings(ApiType::OpenAi);

        let cache_entries = vec![dummy_cache_entry()];

        // We create a few sublime inputs covering all message kinds:
        // Sheet -> weight 1, BuildOutputPanel -> weight 3, ViewSelection -> weight 4,
        // Command -> weight 5 (user command)
        let sublime_inputs = vec![
            dummy_sublime_input("Sheet content", InputKind::Sheet),
            dummy_sublime_input(
                "Output content",
                InputKind::BuildOutputPanel,
            ),
            dummy_sublime_input(
                "View selection",
                InputKind::ViewSelection,
            ),
            dummy_sublime_input("User command last", InputKind::Command),
        ];

        let request = OpenAICompletionRequest::create_openai_completion_request(
            settings,
            cache_entries,
            sublime_inputs,
        );

        // There should be a system message plus one cache entry plus the four sublime inputs.
        assert_eq!(request.messages.len(), 6);

        // Sorted order should follow the weight order, where:
        // SystemMessage (0), SheetContent (1), CacheEntry (2), OutputPaneContent (3), ViewSelection (4), UserCommand (5)
        let weights: Vec<u8> = request
            .messages
            .iter()
            .map(message_weight)
            .collect();
        assert_eq!(weights, vec![0, 1, 2, 3, 4, 5]);

        // Verify the last message is a UserCommand.
        match request
            .messages
            .last()
            .unwrap()
        {
            OpenAIRequestMessage::OpenAIMessage(msg) => {
                assert_eq!(msg.kind, MessageKind::UserCommand);
            }
            OpenAIRequestMessage::OpenAIPlainTextMessage(_) => panic!("Expected OpenAIMessage variant"),
        }
    }

    // Test for the OpenAi branch when the last sublime input yields a FunctionResult.
    #[test]
    fn test_create_completion_request_openaip_function_result_last() {
        let settings = dummy_settings(ApiType::OpenAi);

        let cache_entries = vec![dummy_cache_entry()];

        // Sublime inputs: Sheet (1), BuildOutputPanel (3), ViewSelection (4), FunctionResult (5)
        let sublime_inputs = vec![
            dummy_sublime_input("Sheet content", InputKind::Sheet),
            dummy_sublime_input(
                "Output content",
                InputKind::BuildOutputPanel,
            ),
            dummy_sublime_input(
                "View selection",
                InputKind::ViewSelection,
            ),
            dummy_sublime_input(
                "Function result last",
                InputKind::FunctionResult,
            ),
        ];

        let request = OpenAICompletionRequest::create_openai_completion_request(
            settings,
            cache_entries,
            sublime_inputs,
        );

        assert_eq!(request.messages.len(), 6);

        let weights: Vec<u8> = request
            .messages
            .iter()
            .map(message_weight)
            .collect();
        assert_eq!(weights, vec![0, 1, 2, 3, 4, 5]);

        // Verify the last message is a FunctionResult.
        match request
            .messages
            .last()
            .unwrap()
        {
            OpenAIRequestMessage::OpenAIMessage(msg) => {
                assert_eq!(msg.kind, MessageKind::FunctionResult);
            }
            OpenAIRequestMessage::OpenAIPlainTextMessage(_) => panic!("Expected OpenAIMessage variant"),
        }
    }

    // Test for the PlainText branch when the last sublime input yields a UserCommand.
    #[test]
    fn test_create_completion_request_plaintext_user_command_last() {
        let settings = dummy_settings(ApiType::PlainText);

        let cache_entries = vec![dummy_cache_entry()];

        let sublime_inputs = vec![
            dummy_sublime_input("Sheet content", InputKind::Sheet),
            dummy_sublime_input(
                "Output content",
                InputKind::BuildOutputPanel,
            ),
            dummy_sublime_input(
                "View selection",
                InputKind::ViewSelection,
            ),
            dummy_sublime_input("User command last", InputKind::Command),
        ];

        let request = OpenAICompletionRequest::create_openai_completion_request(
            settings,
            cache_entries,
            sublime_inputs,
        );

        assert_eq!(request.messages.len(), 6);

        let weights: Vec<u8> = request
            .messages
            .iter()
            .map(message_weight)
            .collect();
        assert_eq!(weights, vec![0, 1, 2, 3, 4, 5]);

        // For PlainText branch, messages are built using OpenAIPlainTextMessage.
        // Verify the last message is a UserCommand.
        match request
            .messages
            .last()
            .unwrap()
        {
            OpenAIRequestMessage::OpenAIPlainTextMessage(msg) => {
                assert_eq!(msg.kind, MessageKind::UserCommand);
            }
            OpenAIRequestMessage::OpenAIMessage(_) => panic!("Expected OpenAIPlainTextMessage variant"),
        }
    }

    // Test for the PlainText branch when the last sublime input yields a FunctionResult.
    #[test]
    fn test_create_completion_request_plaintext_function_result_last() {
        let settings = dummy_settings(ApiType::PlainText);

        let cache_entries = vec![dummy_cache_entry()];

        let sublime_inputs = vec![
            dummy_sublime_input("Sheet content", InputKind::Sheet),
            dummy_sublime_input(
                "Output content",
                InputKind::BuildOutputPanel,
            ),
            dummy_sublime_input(
                "View selection",
                InputKind::ViewSelection,
            ),
            dummy_sublime_input(
                "Function result last",
                InputKind::FunctionResult,
            ),
        ];

        let request = OpenAICompletionRequest::create_openai_completion_request(
            settings,
            cache_entries,
            sublime_inputs,
        );

        assert_eq!(request.messages.len(), 6);

        let weights: Vec<u8> = request
            .messages
            .iter()
            .map(message_weight)
            .collect();
        assert_eq!(weights, vec![0, 1, 2, 3, 4, 5]);

        // Verify the last message is a FunctionResult.
        match request
            .messages
            .last()
            .unwrap()
        {
            OpenAIRequestMessage::OpenAIPlainTextMessage(msg) => {
                assert_eq!(msg.kind, MessageKind::FunctionResult);
            }
            OpenAIRequestMessage::OpenAIMessage(_) => panic!("Expected OpenAIPlainTextMessage variant"),
        }
    }

    #[test]
    fn test_stable_sorting_same_weight_preserves_order() {
        // Create settings with no system message so that the output only comes from cache entries and sublime inputs.
        let settings = dummy_settings(ApiType::OpenAi);

        // Create cache entries with distinct content to track their original insertion order.
        fn dummy_cache_entry_with_content(content: &str) -> CacheEntry {
            CacheEntry {
                content: Some(content.to_string()),
                role: Roles::Assistant, // this conversion produces MessageKind::CacheEntry
                tool_call_id: None,
                tool_calls: None,
                thinking: None,
                path: None,
                scope: None,
            }
        }
        let cache_entries = vec![
            dummy_cache_entry_with_content("cache 1"),
            dummy_cache_entry_with_content("cache 2"),
            dummy_cache_entry_with_content("cache 3"),
        ];

        // Create sublime inputs with InputKind::Command (which converts to MessageKind::UserCommand, weight 5)
        // to track their original insertion order.
        let sublime_inputs = vec![
            dummy_sublime_input("sublime 1", InputKind::Command),
            dummy_sublime_input("sublime 2", InputKind::Command),
            dummy_sublime_input("sublime 3", InputKind::Command),
        ];

        let request = OpenAICompletionRequest::create_openai_completion_request(
            settings,
            cache_entries,
            sublime_inputs,
        );

        // The messages vector now is sorted by their weight.
        // Since there is no system message:
        // - Cache entries (weight 2) will appear first.
        // - Sublime inputs (weight 5) come later.
        //
        // Verify that the order of cache entries is preserved, and likewise for sublime inputs.

        // Extract text from cache entry messages (weight 2).
        let cache_texts: Vec<String> = request
            .messages
            .iter()
            .filter_map(|msg| {
                let m = match msg {
                    OpenAIRequestMessage::OpenAIMessage(m) => m,
                    OpenAIRequestMessage::OpenAIPlainTextMessage(_) => return None,
                };
                if m.kind == MessageKind::CacheEntry {
                    m.content
                        .as_ref()
                        .and_then(|contents| {
                            contents
                                .get(0)
                                .and_then(|mc| {
                                    if let ContentWrapper::Text(text) = &mc.content {
                                        Some(text.clone())
                                    } else {
                                        None
                                    }
                                })
                        })
                } else {
                    None
                }
            })
            .collect();

        assert_eq!(
            cache_texts,
            vec![
                "cache 1".to_string(),
                "cache 2".to_string(),
                "cache 3".to_string()
            ]
        );

        // Extract text from sublime input messages.
        let sublime_texts: Vec<String> = request
            .messages
            .iter()
            .filter_map(|msg| {
                // For ApiType::OpenAi we expect these to be OpenAIMessage variants.
                let m = match msg {
                    OpenAIRequestMessage::OpenAIMessage(m) => m,
                    OpenAIRequestMessage::OpenAIPlainTextMessage(_) => return None,
                };
                if m.kind == MessageKind::UserCommand {
                    m.content
                        .as_ref()
                        .and_then(|contents| {
                            contents
                                .get(0)
                                .and_then(|mc| {
                                    if let ContentWrapper::Text(text) = &mc.content {
                                        Some(text.clone())
                                    } else {
                                        None
                                    }
                                })
                        })
                } else {
                    None
                }
            })
            .collect();

        assert_eq!(
            sublime_texts,
            vec![
                "sublime 1".to_string(),
                "sublime 2".to_string(),
                "sublime 3".to_string()
            ]
        );
    }

    fn get_message_text(msg: &OpenAIRequestMessage) -> String {
        match msg {
            OpenAIRequestMessage::OpenAIMessage(m) => {
                m.content
                    .as_ref()
                    .and_then(|v| v.get(0))
                    .and_then(|mc| {
                        if let ContentWrapper::Text(text) = &mc.content {
                            Some(text.clone())
                        } else {
                            None
                        }
                    })
                    .unwrap_or_else(|| "".to_string())
            }
            OpenAIRequestMessage::OpenAIPlainTextMessage(m) => m.content.clone(),
        }
    }

    // Test that verifies the following order is preserved:
    // System → CacheEntry (User) → CacheEntry (Assistant) → UserCommand
    #[test]
    fn test_ordering_system_cache_user_cache_assistant_user_command() {
        let settings = dummy_settings(ApiType::OpenAi);

        // Create two cache entries: one with role User and one with role Assistant.
        let cache_entries = vec![
            dummy_cache_entry_with_role(Roles::User, "cache user"),
            dummy_cache_entry_with_role(Roles::Assistant, "cache assistant"),
        ];

        // Sublime input that yields a UserCommand (weight 5).
        let sublime_inputs = vec![dummy_sublime_input(
            "user command",
            InputKind::Command,
        )];

        let request = OpenAICompletionRequest::create_openai_completion_request(
            settings,
            cache_entries,
            sublime_inputs,
        );

        // Expecting: system message (weight 0), then two cache entries (weight 2),
        // and finally the sublime input (weight 5).
        assert_eq!(request.messages.len(), 4);

        // Check system message.
        match &request.messages[0] {
            OpenAIRequestMessage::OpenAIMessage(msg) => {
                assert_eq!(msg.kind, MessageKind::SystemMessage);
                let text = get_message_text(&request.messages[0]);
                // Since no advertisement is applied here, the system text should equal the settings value.
                assert_eq!(text, "System role");
            }
            _ => panic!("Expected system message to be OpenAIMessage variant"),
        }

        // Check first cache entry (should be the one from User).
        match &request.messages[1] {
            OpenAIRequestMessage::OpenAIMessage(msg) => {
                assert_eq!(msg.kind, MessageKind::CacheEntry);
                let text = get_message_text(&request.messages[1]);
                assert_eq!(text, "cache user");
            }
            _ => panic!("Expected first cache entry to be OpenAIMessage variant"),
        }

        // Check second cache entry (should be the one from Assistant).
        match &request.messages[2] {
            OpenAIRequestMessage::OpenAIMessage(msg) => {
                assert_eq!(msg.kind, MessageKind::CacheEntry);
                let text = get_message_text(&request.messages[2]);
                assert_eq!(text, "cache assistant");
            }
            _ => panic!("Expected second cache entry to be OpenAIMessage variant"),
        }

        // Check sublime input: the last message should be a UserCommand.
        match &request.messages[3] {
            OpenAIRequestMessage::OpenAIMessage(msg) => {
                assert_eq!(msg.kind, MessageKind::UserCommand);
                let text = get_message_text(&request.messages[3]);
                assert_eq!(text, "user command");
            }
            _ => panic!("Expected sublime input message to be OpenAIMessage variant"),
        }
    }

    // Test that verifies the following order is preserved:
    // System → CacheEntry (User) → CacheEntry (Assistant) → ViewSelection
    #[test]
    fn test_ordering_system_cache_user_cache_assistant_view_selection() {
        let settings = dummy_settings(ApiType::OpenAi);

        let cache_entries = vec![
            dummy_cache_entry_with_role(Roles::User, "cache user"),
            dummy_cache_entry_with_role(Roles::Assistant, "cache assistant"),
        ];

        // Sublime input that yields a ViewSelection (weight 4).
        let sublime_inputs = vec![dummy_sublime_input(
            "view selection",
            InputKind::ViewSelection,
        )];

        let request = OpenAICompletionRequest::create_openai_completion_request(
            settings,
            cache_entries,
            sublime_inputs,
        );

        // Expecting: system message, two cache entries, then the sublime input.
        assert_eq!(request.messages.len(), 4);

        // Check system message.
        match &request.messages[0] {
            OpenAIRequestMessage::OpenAIMessage(msg) => {
                assert_eq!(msg.kind, MessageKind::SystemMessage);
                let text = get_message_text(&request.messages[0]);
                assert_eq!(text, "System role");
            }
            _ => panic!("Expected system message to be OpenAIMessage variant"),
        }

        // Check first cache entry (User).
        match &request.messages[1] {
            OpenAIRequestMessage::OpenAIMessage(msg) => {
                assert_eq!(msg.kind, MessageKind::CacheEntry);
                let text = get_message_text(&request.messages[1]);
                assert_eq!(text, "cache user");
            }
            _ => panic!("Expected first cache entry to be OpenAIMessage variant"),
        }

        // Check second cache entry (Assistant).
        match &request.messages[2] {
            OpenAIRequestMessage::OpenAIMessage(msg) => {
                assert_eq!(msg.kind, MessageKind::CacheEntry);
                let text = get_message_text(&request.messages[2]);
                assert_eq!(text, "cache assistant");
            }
            _ => panic!("Expected second cache entry to be OpenAIMessage variant"),
        }

        // Check sublime input: the last message should be a ViewSelection.
        match &request.messages[3] {
            OpenAIRequestMessage::OpenAIMessage(msg) => {
                assert_eq!(msg.kind, MessageKind::ViewSelection);
                let text = get_message_text(&request.messages[3]);
                assert_eq!(text, "view selection");
            }
            _ => panic!("Expected sublime input message to be OpenAIMessage variant"),
        }
    }
}
