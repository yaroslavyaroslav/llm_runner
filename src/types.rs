use pyo3::{pyclass, pymethods};
use serde::{Deserialize, Serialize};
use strum_macros::{Display, EnumString};

use crate::openai_network_types::{AssistantMessage, Roles, ToolCall};

#[allow(unused)]
#[derive(Clone, Copy, Debug)]
pub enum PromptMode {
    View,
    Phantom,
    // OutputPanel, // TODO: review is it necessary
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(crate) struct CacheEntry {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) content: Option<String>,

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
        CacheEntry {
            content: content.content,
            path: content.path,
            scope: content.scope,
            role: Roles::User,
            tool_call: None,
            tool_call_id: None,
        }
    }
}

impl From<AssistantMessage> for CacheEntry {
    fn from(content: AssistantMessage) -> Self {
        let first_tool_call = content
            .tool_calls
            .as_ref()
            .and_then(|calls| calls.first().cloned());
        CacheEntry {
            content: content.content,
            path: None,
            scope: None,
            role: content.role,
            tool_call: first_tool_call,
            tool_call_id: None,
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
        }
    }
}

#[pyclass]
#[derive(Debug, Clone, Deserialize)]
pub struct AssistantSettings {
    #[pyo3(get)]
    pub name: String,

    #[pyo3(get)]
    pub output_mode: OutputMode,

    #[pyo3(get)]
    pub url: String,

    #[pyo3(get)]
    pub chat_model: String,

    #[pyo3(get)]
    pub token: Option<String>,

    #[pyo3(get)]
    pub assistant_role: Option<String>,

    #[pyo3(get)]
    pub temperature: Option<f64>,

    #[pyo3(get)]
    pub max_tokens: Option<usize>,

    #[pyo3(get)]
    pub max_completion_tokens: Option<usize>,

    #[pyo3(get)]
    pub top_p: Option<usize>,

    #[pyo3(get)]
    pub frequency_penalty: Option<usize>,

    #[pyo3(get)]
    pub presence_penalty: Option<usize>,

    #[pyo3(get)]
    pub tools: Option<bool>,

    #[pyo3(get)]
    pub parallel_tool_calls: Option<bool>,

    #[pyo3(get)]
    pub stream: bool,

    #[pyo3(get)]
    pub advertisement: bool,
}

#[pymethods]
impl AssistantSettings {
    #[new]
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (name, output_mode, chat_model, url=None, token=None, assistant_role=None, temperature=None, max_tokens=None, max_completion_tokens=None, top_p=None, frequency_penalty=None, presence_penalty=None, tools=None, parallel_tool_calls=None, stream=None, advertisement=None))]
    pub fn new(
        name: String,
        output_mode: OutputMode,
        chat_model: String,
        url: Option<String>,
        token: Option<String>,
        assistant_role: Option<String>,
        temperature: Option<f64>,
        max_tokens: Option<usize>,
        max_completion_tokens: Option<usize>,
        top_p: Option<usize>,
        frequency_penalty: Option<usize>,
        presence_penalty: Option<usize>,
        tools: Option<bool>,
        parallel_tool_calls: Option<bool>,
        stream: Option<bool>,
        advertisement: Option<bool>,
    ) -> Self {
        let mut default = AssistantSettings::default();

        default.name = name;
        default.output_mode = output_mode;
        default.token = token;
        default.chat_model = chat_model;
        default.url = url.unwrap_or(default.url);
        default.assistant_role = assistant_role.or(default.assistant_role);
        default.temperature = temperature.or(default.temperature);
        default.max_tokens = max_tokens.or(default.max_tokens);
        default.max_completion_tokens = max_completion_tokens.or(default.max_completion_tokens);
        default.top_p = top_p.or(default.top_p);
        default.frequency_penalty = frequency_penalty.or(default.frequency_penalty);
        default.presence_penalty = presence_penalty.or(default.presence_penalty);
        default.tools = tools.or(default.tools);
        default.parallel_tool_calls = parallel_tool_calls.or(default.parallel_tool_calls);
        default.stream = stream.unwrap_or(default.stream);
        default.advertisement = advertisement.unwrap_or(default.advertisement);
        default
    }
}

impl Default for AssistantSettings {
    fn default() -> Self {
        Self {
            name: "Default".to_string(),
            output_mode: OutputMode::Phantom,
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

#[pyclass(eq, eq_int)]
#[derive(EnumString, Display, Debug, Clone, Deserialize, PartialEq)]
#[allow(unused)]
pub enum OutputMode {
    Panel,
    Phantom,
}
