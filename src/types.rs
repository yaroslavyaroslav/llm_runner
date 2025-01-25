use std::{collections::HashMap, str::FromStr};

use pyo3::{pyclass, pymethods, FromPyObject};
use serde::{Deserialize, Serialize};
use strum_macros::{Display, EnumString};

use crate::openai_network_types::{AssistantMessage, Roles, ToolCall};

#[allow(unused)]
#[pyclass(eq, eq_int)]
#[derive(EnumString, Display, Debug, Clone, Deserialize, PartialEq, Serialize)]
pub enum PromptMode {
    #[strum(serialize = "view")]
    View,
    #[strum(serialize = "phantom")]
    Phantom,
    // OutputPanel, // TODO: review is it necessary
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(crate) struct CacheEntry {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) content: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) thinking: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) path: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) scope: Option<String>,

    pub(crate) role: Roles,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) tool_call: Option<ToolCall>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) tool_call_id: Option<String>,
}

impl From<SublimeInputContent> for CacheEntry {
    fn from(content: SublimeInputContent) -> Self {
        // FIXME: Content should be parsed to extract thinking part
        // content should be set without thinking part.
        let thinking = content.content.clone();

        let role = match content.input_kind {
            InputKind::AssistantResponse => Roles::Assistant,
            _ => {
                if content.tool_id.is_some() {
                    Roles::Tool
                } else {
                    Roles::User
                }
            }
        };

        CacheEntry {
            content: content.content,
            thinking,
            path: content.path,
            scope: content.scope,
            role,
            tool_call: None,
            tool_call_id: content.tool_id,
        }
    }
}

impl From<AssistantMessage> for CacheEntry {
    fn from(content: AssistantMessage) -> Self {
        let first_tool_call = content
            .tool_calls
            .as_ref()
            .and_then(|calls| calls.first().cloned());

        // FIXME: Content should be parsed to extract thinking part
        // content should be set without thinking part.
        let thinking = content.content.clone();

        CacheEntry {
            content: content.content,
            thinking,
            path: None,
            scope: None,
            role: content.role,
            tool_call: first_tool_call.clone(),
            tool_call_id: first_tool_call.map(|t| t.id),
        }
    }
}

#[pyclass(eq, eq_int)]
#[derive(EnumString, Display, Debug, Clone, Copy, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum InputKind {
    ViewSelection,
    Command,
    BuildOutputPanel,
    LspOutputPanel,
    Terminus,
    Sheet,
    FunctionResult,
    AssistantResponse,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[pyclass]
pub struct SublimeOutputContent {
    #[pyo3(get)]
    pub content: Option<String>,

    #[pyo3(get)]
    pub role: Roles,
}

impl From<&CacheEntry> for SublimeOutputContent {
    fn from(content: &CacheEntry) -> Self {
        SublimeOutputContent {
            content: content.content.clone(),
            role: content.role.clone(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[pyclass]
pub struct SublimeInputContent {
    #[pyo3(get)]
    pub content: Option<String>,

    #[pyo3(get)]
    pub path: Option<String>,

    #[pyo3(get)]
    pub scope: Option<String>,

    #[pyo3(get)]
    pub input_kind: InputKind,

    pub tool_id: Option<String>,
}

#[pymethods]
impl SublimeInputContent {
    #[new]
    #[pyo3(signature = (input_kind, content=None, path=None, scope=None))]
    pub fn new(
        input_kind: InputKind,
        content: Option<String>,
        path: Option<String>,
        scope: Option<String>,
    ) -> Self {
        SublimeInputContent {
            content,
            path,
            scope,
            input_kind,
            tool_id: None,
        }
    }
}

#[pyclass]
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AssistantSettings {
    #[pyo3(get)]
    pub name: String,

    #[pyo3(get)]
    pub output_mode: PromptMode,

    #[pyo3(get, set)]
    pub url: String,

    #[pyo3(get)]
    pub chat_model: String,

    #[pyo3(get, set)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,

    #[pyo3(get)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assistant_role: Option<String>,

    #[pyo3(get)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,

    #[pyo3(get)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<usize>,

    #[pyo3(get)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_completion_tokens: Option<usize>,

    #[pyo3(get)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<usize>,

    #[pyo3(get)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frequency_penalty: Option<usize>,

    #[pyo3(get)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presence_penalty: Option<usize>,

    #[pyo3(get)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<bool>,

    #[pyo3(get)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parallel_tool_calls: Option<bool>,

    #[pyo3(get)]
    pub stream: bool,

    #[pyo3(get)]
    pub advertisement: bool,
}

#[derive(FromPyObject, Clone)]
pub enum RustyEnum {
    Bool(bool),
    Int(usize),
    Float(f64),
    String(String),
}

#[pymethods]
impl AssistantSettings {
    #[new]
    #[pyo3(signature = (dict))]
    pub fn new(dict: HashMap<String, RustyEnum>) -> Self {
        let mut default = AssistantSettings::default();

        if let Some(RustyEnum::String(value)) = dict.get("name") {
            default.name = value.clone();
        }

        if let Some(RustyEnum::String(value)) = dict.get("output_mode") {
            default.output_mode = PromptMode::from_str(value).unwrap_or(PromptMode::Phantom);
        }

        if let Some(RustyEnum::String(value)) = dict.get("token") {
            default.token = Some(value.clone());
        }
        if let Some(RustyEnum::String(value)) = dict.get("chat_model") {
            default.chat_model = value.clone();
        }

        if let Some(RustyEnum::String(value)) = dict.get("url") {
            default.url = value.clone();
        }

        if let Some(RustyEnum::String(value)) = dict.get("assistant_role") {
            default.assistant_role = Some(value.clone());
        }

        if let Some(RustyEnum::Float(value)) = dict.get("temperature") {
            default.temperature = Some(value.clone().into());
        }

        if let Some(RustyEnum::Int(value)) = dict.get("max_tokens") {
            default.max_tokens = Some(value.clone());
        }

        if let Some(RustyEnum::Int(value)) = dict.get("max_completion_tokens") {
            default.max_completion_tokens = Some(value.clone()); // TODO: This should be self exclusive with max_tokens
        }

        if let Some(RustyEnum::Int(value)) = dict.get("top_p") {
            default.top_p = Some(value.clone());
        }

        if let Some(RustyEnum::Int(value)) = dict.get("frequency_penalty") {
            default.frequency_penalty = Some(value.clone().into());
        }

        if let Some(RustyEnum::Int(value)) = dict.get("presence_penalty") {
            default.presence_penalty = Some(value.clone().into());
        }

        if let Some(RustyEnum::Bool(value)) = dict.get("tools") {
            default.tools = Some(value.clone());
        }

        if let Some(RustyEnum::Bool(value)) = dict.get("parallel_tool_calls") {
            default.parallel_tool_calls = Some(value.clone());
        }

        if let Some(RustyEnum::Bool(value)) = dict.get("stream") {
            default.stream = value.clone();
        }

        if let Some(RustyEnum::Bool(value)) = dict.get("advertisement") {
            default.advertisement = value.clone();
        }

        default
    }
}

impl Default for AssistantSettings {
    fn default() -> Self {
        Self {
            name: "Default".to_string(),
            output_mode: PromptMode::Phantom,
            chat_model: "gpt-4o-mini".to_string(),
            assistant_role: None,
            url: "https://api.openai.com/v1/chat/completions".to_string(),
            token: None,
            temperature: None,
            max_tokens: None,
            max_completion_tokens: None,
            top_p: None,
            frequency_penalty: None,
            presence_penalty: None,
            tools: None,
            parallel_tool_calls: None,
            stream: true,
            advertisement: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_sync() {
        fn is_sync<T: Sync>() {}

        is_sync::<AssistantSettings>();
        is_sync::<SublimeInputContent>();
        is_sync::<InputKind>();
        is_sync::<CacheEntry>();
        is_sync::<PromptMode>();
    }

    #[test]
    fn test_is_send() {
        fn is_send<T: Send>() {}

        is_send::<AssistantSettings>();
        is_send::<SublimeInputContent>();
        is_send::<InputKind>();
        is_send::<CacheEntry>();
        is_send::<PromptMode>();
    }
}
