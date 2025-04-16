use anyhow::Result;
use pyo3::pyclass;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use strum_macros::{Display, EnumString};

use crate::{
    openai_network_types::OpenAIMessageType,
    tools_definition::{FUNCTIONS, OPENAI_DEFINED},
    types::{ApiType, AssistantSettings, CacheEntry, InputKind, Reason, ReasonEffort, SublimeInputContent},
};

#[derive(Debug, Serialize, Deserialize)]
pub struct Response {
    pub id: String,
    pub object: String,
    pub created_at: i64,
    pub status: Option<String>,
    pub error: Option<Value>,
    pub incomplete_details: Option<Value>,
    pub instructions: Option<Value>,
    pub max_output_tokens: Option<Value>,
    pub model: String,
    pub output: Vec<Message>,
    pub parallel_tool_calls: Option<bool>,
    pub previous_response_id: Option<String>,
    pub reasoning: Option<Reasoning>,
    pub store: Option<bool>,
    pub temperature: f64,
    pub text: Option<Text>,
    pub tool_choice: String,
    pub tools: Option<Vec<Value>>,
    pub top_p: f64,
    pub truncation: String,
    pub usage: Option<Usage>,
    pub user: Option<Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Message {
    #[serde(rename = "type")]
    pub r#type: String,
    pub id: String,
    pub status: String,
    pub role: String,
    pub content: Vec<Content>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Content {
    #[serde(rename = "type")]
    pub r#type: OpenAIMessageType,
    pub text: Option<String>,
    pub annotations: Option<Vec<Value>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Reasoning {
    pub effort: Option<Value>,
    pub generate_summary: Option<Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Text {
    pub format: Format,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Format {
    #[serde(rename = "type")]
    pub r#type: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Usage {
    pub input_tokens: u64,
    pub input_tokens_details: InputTokensDetails,
    pub output_tokens: u64,
    pub output_tokens_details: OutputTokensDetails,
    pub total_tokens: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct InputTokensDetails {
    pub cached_tokens: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OutputTokensDetails {
    pub reasoning_tokens: u64,
}
