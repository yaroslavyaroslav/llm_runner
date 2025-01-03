use serde::{Deserialize, Serialize};

// use crate::network_client::OpenAIErrors;
use crate::openai_network_types::{Roles, ToolCall};

#[allow(unused)]
pub enum PromptMode {
    View,
    Phantom,
    // OutputPanel, // TODO: review is it necessary
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(crate) struct CacheEntry {
    pub(crate) content: Option<String>,
    pub(crate) path: Option<String>,
    pub(crate) scope: Option<String>,
    pub(crate) role: Roles,
    pub(crate) tool_call: Option<ToolCall>,
    pub(crate) tool_call_id: Option<String>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[allow(unused)]
#[serde(rename_all = "lowercase")]
pub(crate) enum InputKind {
    ViewSelection,
    Command,
    BuildOutputPanel,
    LSPOutputPanel,
    Terminus,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(unused)]
pub(crate) struct SublimeInputContent {
    pub(crate) content: Option<String>,
    pub(crate) path: Option<String>,
    pub(crate) scope: Option<String>,
    pub(crate) input_kind: InputKind,
}

#[derive(Debug, Clone)]
pub struct AssistantSettings {
    pub name: String,
    pub output_mode: OutputMode,
    pub token: String,
    pub url: String,
    pub chat_model: String,
    pub assistant_role: Option<String>,
    pub temperature: Option<usize>,
    pub max_tokens: Option<usize>,
    pub max_completion_tokens: Option<usize>,
    pub top_p: Option<usize>,
    pub frequency_penalty: Option<usize>,
    pub presence_penalty: Option<usize>,
    pub tools: Option<bool>,
    pub parallel_tool_calls: Option<bool>,
    pub stream: bool,
    pub advertisement: bool,
}

impl Default for AssistantSettings {
    fn default() -> Self {
        Self {
            name: "Default".to_string(),
            output_mode: OutputMode::Phantom,
            chat_model: "gpt-4o".to_string(),
            assistant_role: None,
            url: "None".to_string(),
            token: "None".to_string(),
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

#[derive(Debug, Clone)]
#[allow(unused)]
pub enum OutputMode {
    Panel,
    Phantom,
}
