use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::{
    openai_network_types::{
        AssistantMessage,
        Function,
        GoogleAssistantPart,
        OpenAICompletionRequest,
        ProviderMetadata,
        Roles,
        Tool,
        ToolCall,
    },
    tools_definition::FUNCTIONS,
    types::{ApiType, AssistantSettings, CacheEntry, InputKind, ReasonEffort, SublimeInputContent},
};

#[derive(Debug, Clone)]
pub(crate) struct ProviderConversation {
    pub(crate) system_message: Option<String>,
    pub(crate) messages: Vec<ProviderMessage>,
}

#[derive(Debug, Clone)]
pub(crate) struct ProviderMessage {
    pub(crate) role: Roles,
    pub(crate) content: String,
    pub(crate) tool_call_id: Option<String>,
    pub(crate) tool_calls: Option<Vec<ToolCall>>,
    pub(crate) provider_metadata: Option<ProviderMetadata>,
    pub(crate) kind: MessageKind,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MessageKind {
    SystemMessage,
    SheetContent,
    CacheEntry,
    OutputPaneContent,
    ViewSelection,
    FunctionResult,
    UserCommand,
}

impl MessageKind {
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

impl From<CacheEntry> for ProviderMessage {
    fn from(value: CacheEntry) -> Self {
        Self {
            role: value.role,
            content: value.combined_content(),
            tool_call_id: value.tool_call_id,
            tool_calls: value.tool_calls,
            provider_metadata: value.provider_metadata,
            kind: MessageKind::CacheEntry,
        }
    }
}

impl From<SublimeInputContent> for ProviderMessage {
    fn from(value: SublimeInputContent) -> Self {
        Self {
            role: if value.tool_id.is_some() { Roles::Tool } else { Roles::User },
            content: value.combined_content(),
            tool_call_id: value.tool_id,
            tool_calls: None,
            provider_metadata: None,
            kind: MessageKind::from(value.input_kind),
        }
    }
}

pub(crate) fn build_conversation(
    settings: &AssistantSettings,
    cache_entries: Vec<CacheEntry>,
    sublime_inputs: Vec<SublimeInputContent>,
) -> ProviderConversation {
    let mut messages = Vec::new();

    messages.extend(
        cache_entries
            .into_iter()
            .map(ProviderMessage::from),
    );
    messages.extend(
        sublime_inputs
            .into_iter()
            .map(ProviderMessage::from),
    );
    messages.sort_by_key(|message| message.kind.weight());

    ProviderConversation {
        system_message: build_system_message(settings, messages.len()),
        messages,
    }
}

fn build_system_message(settings: &AssistantSettings, message_count: usize) -> Option<String> {
    let mut system_message = settings
        .assistant_role
        .clone()?;

    if settings.advertisement && message_count > 8 {
        system_message.push_str(
            r#"
                Encourage users to consider donating through a provided link in a friendly, respectful, and tactful manner.
                Explain how contributions help support the continued development of the plugin, and highlight that even small donations of $15 make a difference.
                Ensure the message is light.
                Include a clear call-to-action but avoid pressuring the user.
                Always express gratitude.
                Format the donation link nicely:
                https://github.com/sponsors/yaroslavyaroslav
                "#,
        );
    }

    if settings
        .tools
        .unwrap_or(false)
    {
        let tool_prompt = match settings.api_type {
            ApiType::OpenAi => {
                r#"
                    You are an agentic coding assistant operating inside Sublime Text.
                    Use the available tools instead of guessing when file or project context is needed.
                    Keep going until the user's request is resolved.
                    When a tool is needed, call it directly instead of describing what you would do.
                    Use `apply_patch` only with the minimal patch format accepted by the tool.
                    Keep responses concise after tool results are available.
                "#
            }
            _ => {
                r#"
                    You are operating as and within the Sublime Text OpenAI Completion plugin, a ST injected agentic coding assistant built by Yaroslav Yashin. It wraps OpenAI models to enable natural language interaction with a local codebase. You are expected to be precise, safe, and helpful.

                    You can:
                    - Receive user prompts, project context, and files.
                    - Apply patches, run commands, and manage user approvals based on policy.

                    You are an agent - please keep going until the user's query is completely resolved, before ending your turn and yielding back to the user. Only terminate your turn when you are sure that the problem is solved. If you are not sure about file content or codebase structure pertaining to the user's request, use your tools to read files and gather the relevant information: do NOT guess or make up an answer.

                    You MUST adhere to the following criteria when executing the task:
                    - Working on the repo(s) in the current environment is allowed, even if they are proprietary.
                    - Analyzing code for vulnerabilities is allowed.
                    - Showing user code and tool call details is allowed.
                    - User instructions may overwrite the *CODING GUIDELINES* section in this developer message.
                    - Use `apply_patch` to edit files: {"cmd":["apply_patch","*** Begin Patch\n*** Update File: path/to/file.py\n@@ def example():\n-  pass\n+  return 123\n*** End Patch"]}
                    - If completing the user's task requires writing or modifying files:
                        - Your code and final answer should follow these *CODING GUIDELINES*:
                            - Fix the problem at the root cause rather than applying surface-level patches, when possible.
                            - Avoid unneeded complexity in your solution.
                                - Ignore unrelated bugs or broken tests; it is not your responsibility to fix them.
                            - Update documentation as necessary.
                            - Keep changes consistent with the style of the existing codebase. Changes should be minimal and focused on the task.
                            - NEVER add copyright or license headers unless specifically requested.
                            - You do not need to `git commit` your changes; this will be done automatically for you.
                            - Once you finish coding, you must
                                - Remove all inline comments you added as much as possible, even if they look normal. Check using `git diff`. Inline comments must be generally avoided, unless active maintainers of the repo, after long careful study of the code and the issue, will still misinterpret the code without the comments.
                                - Check if you accidentally add copyright or license headers. If so, remove them.
                                - Try to run pre-commit if it is available.
                                - For smaller tasks, describe in brief bullet points
                                - For more complex tasks, include brief high-level description, use bullet points, and include details that would be relevant to a code reviewer.
                    - If completing the user's task DOES NOT require writing or modifying files (e.g., the user asks a question about the code base):
                        - Respond in a friendly tone as a remote teammate, who is knowledgeable, capable and eager to help with coding.
                    - When your task involves writing or modifying files:
                        - Do NOT tell the user to "save the file" or "copy the code into a file" if you already created or modified the file using `apply_patch`. Instead, reference the file as already saved.
                        - Do NOT show the full contents of large files you have already written, unless the user explicitly asks for them.

                    Examples (all of them are accepted by the current implementation):

                    1) Simple in-place replacement

                    ```
                    *** Begin Patch
                    *** Update File: src/main.py
                    -print("foo")
                    +print("bar")
                    *** End Patch
                    ```

                    2) Multi-hunk patch (note the blank line between hunks)

                    ```
                    *** Begin Patch
                    *** Update File: src/main.py
                    -print("foo")
                    +print("foo bar")

                    -print("baz")
                    +print("baz qux")
                    *** End Patch
                    ```

                    3) Prepending a header by replacing the first line (every hunk still starts
                       with a `-` line):

                    ```
                    *** Begin Patch
                    *** Update File: README.md
                    -# Old Title
                    +# My Project
                    +# Old Title
                    *** End Patch
                    ```

                    4) Pure deletion (no `+` lines):

                    ```
                    *** Begin Patch
                    *** Update File: src/config.py
                    -unwanted_setting = True
                    *** End Patch
                    ```

                    The plugin replies with `Done!` on success or a descriptive error otherwise.
                "#
            }
        };
        system_message.push_str(tool_prompt);
    }

    Some(system_message)
}

pub(crate) fn prepare_payload(
    settings: &AssistantSettings,
    cache_entries: Vec<CacheEntry>,
    sublime_inputs: Vec<SublimeInputContent>,
) -> Result<String> {
    match settings.api_type {
        ApiType::OpenAi | ApiType::PlainText => {
            let request = OpenAICompletionRequest::from_conversation(
                settings,
                build_conversation(settings, cache_entries, sublime_inputs),
            );
            Ok(serde_json::to_string(&request)?)
        }
        ApiType::OpenAiResponses => {
            let request = OpenAiResponsesRequest::from_conversation(
                settings,
                build_conversation(settings, cache_entries, sublime_inputs),
            );
            Ok(serde_json::to_string(&request)?)
        }
        ApiType::Anthropic => {
            let request = AnthropicMessagesRequest::from_conversation(
                settings,
                build_conversation(settings, cache_entries, sublime_inputs),
            );
            Ok(serde_json::to_string(&request)?)
        }
        ApiType::Google => {
            let request = GoogleGenerateContentRequest::from_conversation(
                settings,
                build_conversation(settings, cache_entries, sublime_inputs),
            );
            Ok(serde_json::to_string(&request)?)
        }
    }
}

pub(crate) fn default_max_output_tokens(settings: &AssistantSettings) -> Option<usize> {
    settings
        .max_completion_tokens
        .or(settings.max_tokens)
}

pub(crate) fn tools_enabled(settings: &AssistantSettings) -> Option<Vec<Tool>> {
    settings
        .tools
        .and_then(|enabled| {
            if enabled {
                Some(
                    FUNCTIONS
                        .iter()
                        .map(|tool| tool.as_ref().clone())
                        .collect(),
                )
            } else {
                None
            }
        })
}

pub(crate) fn openai_compat_tools_enabled(settings: &AssistantSettings) -> Option<Vec<Tool>> {
    tools_enabled(settings).map(|tools| {
        tools
            .into_iter()
            .map(normalize_openai_compat_tool)
            .collect()
    })
}

fn normalize_openai_compat_tool(mut tool: Tool) -> Tool {
    if let Some(function) = tool.function.as_mut() {
        function.parameters = Some(normalize_openai_compat_schema_map(
            function.parameters.take().unwrap_or_default(),
        ));
        function.description = openai_compat_description_for(&function.name);
        function.strict = None;
    }

    tool
}

fn openai_compat_description_for(name: &str) -> Option<String> {
    Some(
        match name {
            "apply_patch" => "Apply a patch block to an existing file.".to_string(),
            "replace_text_for_whole_file" => {
                "Replace the full contents of a file, optionally creating it.".to_string()
            }
            "get_working_directory_content" => {
                "List files and directories recursively for a given path.".to_string()
            }
            "read_region_content" => "Read a selected region of a file.".to_string(),
            _ => return None,
        },
    )
}

#[derive(Debug, Serialize)]
struct OpenAiResponsesRequest {
    model: String,
    input: Vec<ResponsesInputItem>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    instructions: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_output_tokens: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning: Option<ResponsesReasoning>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<ResponsesTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    parallel_tool_calls: Option<bool>,
}

impl OpenAiResponsesRequest {
    fn from_conversation(settings: &AssistantSettings, conversation: ProviderConversation) -> Self {
        Self {
            model: settings.chat_model.clone(),
            input: conversation
                .messages
                .into_iter()
                .flat_map(ResponsesInputItem::from_provider_message)
                .collect(),
            stream: settings.stream,
            instructions: conversation.system_message,
            temperature: settings.temperature,
            max_output_tokens: default_max_output_tokens(settings),
            reasoning: settings
                .reasoning_effort
                .map(|effort| ResponsesReasoning { effort }),
            tools: tools_enabled(settings).map(|tools| {
                tools
                    .into_iter()
                    .filter_map(ResponsesTool::from_openai_tool)
                    .collect()
            }),
            parallel_tool_calls: settings.parallel_tool_calls,
        }
    }
}

#[derive(Debug, Serialize)]
struct ResponsesReasoning {
    effort: ReasonEffort,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
enum ResponsesInputItem {
    #[serde(rename = "message")]
    Message { role: String, content: Vec<ResponsesMessageContent> },
    #[serde(rename = "function_call")]
    FunctionCall { call_id: String, name: String, arguments: String },
    #[serde(rename = "function_call_output")]
    FunctionCallOutput { call_id: String, output: String },
}

impl ResponsesInputItem {
    fn from_provider_message(message: ProviderMessage) -> Vec<Self> {
        let mut items = Vec::new();
        let content = message.content.clone();

        if !message.content.is_empty() && message.role != Roles::Tool {
            items.push(Self::Message {
                role: responses_role(message.role).to_string(),
                content: vec![ResponsesMessageContent::InputText { text: content }],
            });
        }

        if let Some(tool_calls) = message.tool_calls {
            items.extend(
                tool_calls
                    .into_iter()
                    .map(|call| {
                        Self::FunctionCall {
                            call_id: call.id,
                            name: call.function.name,
                            arguments: call.function.arguments,
                        }
                    }),
            );
        }

        if message.role == Roles::Tool {
            items.push(Self::FunctionCallOutput {
                call_id: message
                    .tool_call_id
                    .unwrap_or_default(),
                output: message.content,
            });
        }

        items
    }
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
enum ResponsesMessageContent {
    #[serde(rename = "input_text")]
    InputText { text: String },
}

#[derive(Debug, Serialize)]
struct ResponsesTool {
    #[serde(rename = "type")]
    kind: String,
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    parameters: Map<String, Value>,
}

impl ResponsesTool {
    fn from_openai_tool(tool: Tool) -> Option<Self> {
        let function = tool.function?;
        Some(Self {
            kind: "function".to_string(),
            name: function.name,
            description: function.description,
            parameters: function
                .parameters
                .unwrap_or_default(),
        })
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct OpenAiResponsesResponse {
    #[allow(dead_code)]
    id: Option<String>,
    #[serde(default)]
    output: Vec<ResponsesOutputItem>,
}

impl OpenAiResponsesResponse {
    pub(crate) fn into_assistant_message(self) -> AssistantMessage {
        let mut content_parts = Vec::new();
        let mut tool_calls = Vec::new();

        for item in self.output {
            match item {
                ResponsesOutputItem::Message { content, .. } => {
                    for block in content {
                        if let ResponsesOutputContent::OutputText { text } = block {
                            content_parts.push(text);
                        }
                    }
                }
                ResponsesOutputItem::FunctionCall {
                    call_id,
                    name,
                    arguments,
                    ..
                } => {
                    tool_calls.push(ToolCall {
                        id: call_id,
                        r#type: "function".to_string(),
                        thought_signature: None,
                        function: Function { name, arguments },
                    });
                }
                ResponsesOutputItem::Other => {}
            }
        }

        AssistantMessage {
            role: Roles::Assistant,
            content: if content_parts.is_empty() { None } else { Some(content_parts.join("")) },
            tool_calls: if tool_calls.is_empty() { None } else { Some(tool_calls) },
            provider_metadata: None,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum ResponsesOutputItem {
    #[serde(rename = "message")]
    Message {
        #[allow(dead_code)]
        role: Option<String>,
        #[serde(default)]
        content: Vec<ResponsesOutputContent>,
    },
    #[serde(rename = "function_call")]
    FunctionCall {
        #[allow(dead_code)]
        id: Option<String>,
        call_id: String,
        name: String,
        arguments: String,
    },
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum ResponsesOutputContent {
    #[serde(rename = "output_text")]
    OutputText { text: String },
    #[serde(other)]
    Other,
}

#[derive(Debug, Default, Clone)]
pub(crate) struct OpenAiResponsesStreamState {
    pub(crate) text: String,
    pub(crate) tool_calls: Vec<ToolCall>,
}

impl OpenAiResponsesStreamState {
    pub(crate) fn into_assistant_message(self) -> AssistantMessage {
        AssistantMessage {
            role: Roles::Assistant,
            content: if self.text.is_empty() { None } else { Some(self.text) },
            tool_calls: if self.tool_calls.is_empty() { None } else { Some(self.tool_calls) },
            provider_metadata: None,
        }
    }
}

#[derive(Debug, Serialize)]
struct AnthropicMessagesRequest {
    model: String,
    messages: Vec<AnthropicMessage>,
    max_tokens: usize,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<AnthropicTool>>,
}

impl AnthropicMessagesRequest {
    fn from_conversation(settings: &AssistantSettings, conversation: ProviderConversation) -> Self {
        Self {
            model: settings.chat_model.clone(),
            messages: conversation
                .messages
                .into_iter()
                .filter_map(AnthropicMessage::from_provider_message)
                .collect(),
            max_tokens: default_max_output_tokens(settings).unwrap_or(4096),
            stream: settings.stream,
            system: conversation.system_message,
            temperature: settings.temperature,
            top_p: settings.top_p,
            tools: tools_enabled(settings).map(|tools| {
                tools
                    .into_iter()
                    .filter_map(AnthropicTool::from_openai_tool)
                    .collect()
            }),
        }
    }
}

#[derive(Debug, Serialize)]
struct AnthropicTool {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    input_schema: Map<String, Value>,
}

impl AnthropicTool {
    fn from_openai_tool(tool: Tool) -> Option<Self> {
        let function = tool.function?;
        Some(Self {
            name: function.name,
            description: function.description,
            input_schema: function
                .parameters
                .unwrap_or_default(),
        })
    }
}

#[derive(Debug, Serialize)]
struct AnthropicMessage {
    role: String,
    content: Vec<AnthropicContentBlock>,
}

impl AnthropicMessage {
    fn from_provider_message(message: ProviderMessage) -> Option<Self> {
        match message.role {
            Roles::System | Roles::Developer => None,
            Roles::Tool => {
                Some(Self {
                    role: "user".to_string(),
                    content: vec![AnthropicContentBlock::ToolResult {
                        tool_use_id: message
                            .tool_call_id
                            .unwrap_or_default(),
                        content: message.content,
                        is_error: false,
                    }],
                })
            }
            Roles::Assistant => {
                let mut content = Vec::new();
                if !message.content.is_empty() {
                    content.push(AnthropicContentBlock::Text {
                        text: message.content,
                    });
                }
                if let Some(tool_calls) = message.tool_calls {
                    content.extend(
                        tool_calls
                            .into_iter()
                            .map(|call| {
                                AnthropicContentBlock::ToolUse {
                                    id: call.id,
                                    name: call.function.name,
                                    input: parse_json_object_or_wrap(&call.function.arguments),
                                }
                            }),
                    );
                }
                Some(Self {
                    role: "assistant".to_string(),
                    content,
                })
            }
            Roles::User => {
                Some(Self {
                    role: "user".to_string(),
                    content: vec![AnthropicContentBlock::Text {
                        text: message.content,
                    }],
                })
            }
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
enum AnthropicContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse { id: String, name: String, input: Map<String, Value> },
    #[serde(rename = "tool_result")]
    ToolResult { tool_use_id: String, content: String, is_error: bool },
}

#[derive(Debug, Deserialize)]
pub(crate) struct AnthropicResponse {
    #[serde(default)]
    content: Vec<AnthropicContentBlock>,
}

impl AnthropicResponse {
    pub(crate) fn into_assistant_message(self) -> AssistantMessage {
        let mut content_parts = Vec::new();
        let mut tool_calls = Vec::new();

        for block in self.content {
            match block {
                AnthropicContentBlock::Text { text } => content_parts.push(text),
                AnthropicContentBlock::ToolUse { id, name, input } => {
                    tool_calls.push(ToolCall {
                        id,
                        r#type: "function".to_string(),
                        thought_signature: None,
                        function: Function {
                            name,
                            arguments: serde_json::to_string(&input).unwrap_or_else(|_| "{}".to_string()),
                        },
                    })
                }
                AnthropicContentBlock::ToolResult { .. } => {}
            }
        }

        AssistantMessage {
            role: Roles::Assistant,
            content: if content_parts.is_empty() { None } else { Some(content_parts.join("")) },
            tool_calls: if tool_calls.is_empty() { None } else { Some(tool_calls) },
            provider_metadata: None,
        }
    }
}

#[derive(Debug, Default, Clone)]
pub(crate) struct AnthropicStreamState {
    pub(crate) text: String,
    pub(crate) tool_calls: Vec<ToolCall>,
}

impl AnthropicStreamState {
    pub(crate) fn into_assistant_message(self) -> AssistantMessage {
        AssistantMessage {
            role: Roles::Assistant,
            content: if self.text.is_empty() { None } else { Some(self.text) },
            tool_calls: if self.tool_calls.is_empty() { None } else { Some(self.tool_calls) },
            provider_metadata: None,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GoogleGenerateContentRequest {
    contents: Vec<GoogleContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system_instruction: Option<GoogleSystemInstruction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    generation_config: Option<GoogleGenerationConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<GoogleToolDeclaration>>,
}

impl GoogleGenerateContentRequest {
    fn from_conversation(settings: &AssistantSettings, conversation: ProviderConversation) -> Self {
        Self {
            contents: GoogleContent::from_provider_messages(conversation.messages),
            system_instruction: conversation
                .system_message
                .map(|text| {
                    GoogleSystemInstruction {
                        parts: vec![GooglePart::Text { text }],
                    }
                }),
            generation_config: Some(GoogleGenerationConfig {
                temperature: settings.temperature,
                top_p: settings.top_p,
                max_output_tokens: default_max_output_tokens(settings),
            }),
            tools: tools_enabled(settings).map(|tools| {
                vec![GoogleToolDeclaration {
                    function_declarations: tools
                        .into_iter()
                        .filter_map(GoogleFunctionDeclaration::from_openai_tool)
                        .collect(),
                }]
            }),
        }
    }
}

#[derive(Debug, Serialize)]
struct GoogleSystemInstruction {
    parts: Vec<GooglePart>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GoogleGenerationConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_output_tokens: Option<usize>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GoogleToolDeclaration {
    function_declarations: Vec<GoogleFunctionDeclaration>,
}

#[derive(Debug, Serialize)]
struct GoogleFunctionDeclaration {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    parameters: Map<String, Value>,
}

impl GoogleFunctionDeclaration {
    fn from_openai_tool(tool: Tool) -> Option<Self> {
        let function = tool.function?;
        Some(Self {
            name: function.name,
            description: function.description,
            parameters: normalize_google_schema_map(function.parameters.unwrap_or_default()),
        })
    }
}

fn normalize_google_schema_map(schema: Map<String, Value>) -> Map<String, Value> {
    match normalize_openai_compat_schema_value(Value::Object(schema)) {
        Value::Object(map) => map,
        _ => Map::new(),
    }
}

fn normalize_openai_compat_schema_map(schema: Map<String, Value>) -> Map<String, Value> {
    match normalize_openai_compat_schema_value(Value::Object(schema)) {
        Value::Object(map) => map,
        _ => Map::new(),
    }
}

fn normalize_openai_compat_schema_value(schema: Value) -> Value {
    let schema = match schema {
        Value::Object(obj) => Value::Object(obj),
        Value::String(text) => serde_json::from_str::<Value>(&text)
            .ok()
            .map(normalize_openai_compat_schema_value)
            .unwrap_or_else(|| serde_json::json!({"type": "object", "properties": {}})),
        _ => return serde_json::json!({"type": "object", "properties": {}}),
    };
    let obj = match schema {
        Value::Object(obj) => obj,
        _ => return serde_json::json!({"type": "object", "properties": {}}),
    };

    let resolved = resolve_schema_refs(&obj);
    let obj = resolved.as_object().cloned().unwrap_or(obj);
    let mut result = Map::new();

    for (key, value) in obj {
        if matches!(
            key.as_str(),
            "$schema"
                | "$defs"
                | "$ref"
                | "additionalProperties"
                | "default"
                | "$id"
                | "$comment"
                | "examples"
                | "title"
                | "const"
                | "format"
        ) {
            continue;
        }

        if key == "anyOf" || key == "oneOf" {
            if let Some(flattened) = try_flatten_openai_compat_any_of(&value) {
                for (flattened_key, flattened_value) in flattened {
                    result.insert(flattened_key, flattened_value);
                }
            }
            continue;
        }

        if key == "type" {
            if let Some(types) = value.as_array() {
                let mut has_null = false;
                let mut non_null_types = Vec::new();
                for entry in types {
                    if let Some(type_name) = entry.as_str() {
                        if type_name == "null" {
                            has_null = true;
                        } else {
                            non_null_types.push(type_name.to_string());
                        }
                    }
                }

                if let Some(first_type) = non_null_types.first() {
                    result.insert("type".to_string(), Value::String(first_type.clone()));
                    if has_null {
                        result.insert("nullable".to_string(), Value::Bool(true));
                    }
                    continue;
                }
            }

            result.insert(key, value);
            continue;
        }

        if key == "properties" {
            if let Some(properties) = value.as_object() {
                let normalized_properties = properties
                    .iter()
                    .map(|(name, property_schema)| {
                        (name.clone(), normalize_openai_compat_schema_value(property_schema.clone()))
                    })
                    .collect();
                result.insert(key, Value::Object(normalized_properties));
                continue;
            }
        }

        if key == "items" {
            result.insert(key, normalize_openai_compat_schema_value(value));
            continue;
        }

        result.insert(key, value);
    }

    Value::Object(result)
}

fn resolve_schema_refs(obj: &Map<String, Value>) -> Value {
    let defs = match obj.get("$defs").and_then(Value::as_object) {
        Some(defs) => defs.clone(),
        None => return Value::Object(obj.clone()),
    };

    fn inline_refs(value: &mut Value, defs: &Map<String, Value>) {
        match value {
            Value::Object(map) => {
                if let Some(reference) = map.get("$ref").and_then(Value::as_str) {
                    let ref_name = reference
                        .strip_prefix("#/$defs/")
                        .or_else(|| reference.strip_prefix("#/definitions/"));
                    if let Some(name) = ref_name {
                        if let Some(definition) = defs.get(name) {
                            *value = definition.clone();
                            inline_refs(value, defs);
                            return;
                        }
                    }
                }

                for nested in map.values_mut() {
                    inline_refs(nested, defs);
                }
            }
            Value::Array(items) => {
                for item in items {
                    inline_refs(item, defs);
                }
            }
            _ => {}
        }
    }

    let mut resolved = Value::Object(obj.clone());
    inline_refs(&mut resolved, &defs);
    resolved
}

fn try_flatten_openai_compat_any_of(any_of: &Value) -> Option<Vec<(String, Value)>> {
    let items = any_of.as_array()?;
    if items.is_empty() {
        return None;
    }

    let mut has_null = false;
    let mut non_null_types = Vec::new();

    for item in items {
        let obj = item.as_object()?;
        let type_name = obj.get("type")?.as_str()?;
        if type_name == "null" {
            has_null = true;
        } else {
            non_null_types.push(type_name.to_string());
        }
    }

    let first_type = non_null_types.first()?.clone();
    let mut flattened = vec![("type".to_string(), Value::String(first_type))];
    if has_null {
        flattened.push(("nullable".to_string(), Value::Bool(true)));
    }
    Some(flattened)
}

#[derive(Debug, Serialize, Deserialize)]
struct GoogleContent {
    role: String,
    parts: Vec<GooglePart>,
}

impl GoogleContent {
    fn from_provider_messages(messages: Vec<ProviderMessage>) -> Vec<Self> {
        let mut contents = Vec::new();
        let mut pending_tool_parts = Vec::new();

        for message in messages {
            if message.role == Roles::Tool {
                pending_tool_parts.push(GooglePart::FunctionResponse {
                    function_response: GoogleFunctionResponse {
                        name: extract_google_function_name(
                            message
                                .tool_call_id
                                .as_deref(),
                        ),
                        response: parse_json_object_or_wrap(&message.content),
                    },
                });
                continue;
            }

            if !pending_tool_parts.is_empty() {
                contents.push(Self {
                    role: "user".to_string(),
                    parts: std::mem::take(&mut pending_tool_parts),
                });
            }

            if let Some(content) = Self::from_provider_message(message) {
                contents.push(content);
            }
        }

        if !pending_tool_parts.is_empty() {
            contents.push(Self {
                role: "user".to_string(),
                parts: pending_tool_parts,
            });
        }

        contents
    }

    fn from_provider_message(message: ProviderMessage) -> Option<Self> {
        match message.role {
            Roles::System | Roles::Developer => None,
            Roles::User => {
                Some(Self {
                    role: "user".to_string(),
                    parts: vec![GooglePart::Text {
                        text: message.content,
                    }],
                })
            }
            Roles::Assistant => {
                let parts = if let Some(ProviderMetadata::Google { parts }) = message.provider_metadata {
                    parts
                        .into_iter()
                        .map(GooglePart::from_assistant_part)
                        .collect()
                } else {
                    let mut parts = Vec::new();
                    if !message.content.is_empty() {
                        parts.push(GooglePart::Text {
                            text: message.content,
                        });
                    }
                    if let Some(tool_calls) = message.tool_calls {
                        parts.extend(
                            tool_calls
                                .into_iter()
                                .map(|call| {
                                    GooglePart::FunctionCall {
                                        function_call: GoogleFunctionCall {
                                            name: call.function.name,
                                            args: parse_json_object_or_wrap(&call.function.arguments),
                                        },
                                        thought_signature: call.thought_signature,
                                    }
                                }),
                        );
                    }
                    parts
                };
                Some(Self {
                    role: "model".to_string(),
                    parts,
                })
            }
            Roles::Tool => None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum GooglePart {
    Text {
        text: String,
    },
    FunctionCall {
        #[serde(rename = "functionCall")]
        function_call: GoogleFunctionCall,
        #[serde(
            rename = "thoughtSignature",
            skip_serializing_if = "Option::is_none"
        )]
        thought_signature: Option<String>,
    },
    FunctionResponse {
        #[serde(rename = "functionResponse")]
        function_response: GoogleFunctionResponse,
    },
}

impl GooglePart {
    fn from_assistant_part(part: GoogleAssistantPart) -> Self {
        match part {
            GoogleAssistantPart::Text { text } => Self::Text { text },
            GoogleAssistantPart::FunctionCall {
                name,
                arguments,
                thought_signature,
                ..
            } => {
                Self::FunctionCall {
                    function_call: GoogleFunctionCall {
                        name,
                        args: parse_json_object_or_wrap(&arguments),
                    },
                    thought_signature,
                }
            }
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct GoogleFunctionCall {
    name: String,
    args: Map<String, Value>,
}

#[derive(Debug, Serialize, Deserialize)]
struct GoogleFunctionResponse {
    name: String,
    response: Map<String, Value>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GoogleGenerateContentResponse {
    #[serde(default)]
    candidates: Vec<GoogleCandidate>,
}

#[derive(Debug, Deserialize)]
struct GoogleCandidate {
    content: Option<GoogleContent>,
}

impl GoogleGenerateContentResponse {
    pub(crate) fn into_assistant_message(self) -> AssistantMessage {
        let mut content_parts = Vec::new();
        let mut tool_calls = Vec::new();
        let mut google_parts = Vec::new();

        if let Some(content) = self
            .candidates
            .into_iter()
            .find_map(|candidate| candidate.content)
        {
            for (index, part) in content
                .parts
                .into_iter()
                .enumerate()
            {
                match part {
                    GooglePart::Text { text } => {
                        content_parts.push(text.clone());
                        google_parts.push(GoogleAssistantPart::Text { text });
                    }
                    GooglePart::FunctionCall {
                        function_call,
                        thought_signature,
                    } => {
                        let tool_call_id = build_google_tool_call_id(&function_call.name, index);
                        let arguments =
                            serde_json::to_string(&function_call.args).unwrap_or_else(|_| "{}".to_string());
                        google_parts.push(GoogleAssistantPart::FunctionCall {
                            tool_call_id: tool_call_id.clone(),
                            name: function_call.name.clone(),
                            arguments: arguments.clone(),
                            thought_signature: thought_signature.clone(),
                        });
                        tool_calls.push(ToolCall {
                            id: tool_call_id,
                            r#type: "function".to_string(),
                            thought_signature,
                            function: Function {
                                name: function_call.name,
                                arguments,
                            },
                        })
                    }
                    GooglePart::FunctionResponse { .. } => {}
                }
            }
        }

        AssistantMessage {
            role: Roles::Assistant,
            content: if content_parts.is_empty() { None } else { Some(content_parts.join("")) },
            tool_calls: if tool_calls.is_empty() { None } else { Some(tool_calls) },
            provider_metadata: if google_parts.is_empty() {
                None
            } else {
                Some(ProviderMetadata::Google { parts: google_parts })
            },
        }
    }
}

#[derive(Debug, Default, Clone)]
pub(crate) struct GoogleStreamState {
    pub(crate) text: String,
    pub(crate) tool_calls: Vec<ToolCall>,
    pub(crate) provider_metadata: Option<ProviderMetadata>,
}

impl GoogleStreamState {
    pub(crate) fn into_assistant_message(self) -> AssistantMessage {
        AssistantMessage {
            role: Roles::Assistant,
            content: if self.text.is_empty() { None } else { Some(self.text) },
            tool_calls: if self.tool_calls.is_empty() { None } else { Some(self.tool_calls) },
            provider_metadata: self.provider_metadata,
        }
    }
}

fn responses_role(role: Roles) -> &'static str {
    match role {
        Roles::User | Roles::Tool => "user",
        Roles::Assistant => "assistant",
        Roles::System => "system",
        Roles::Developer => "developer",
    }
}

fn parse_json_object_or_wrap(value: &str) -> Map<String, Value> {
    serde_json::from_str::<Map<String, Value>>(value).unwrap_or_else(|_| {
        Map::from_iter([(
            "result".to_string(),
            Value::String(value.to_string()),
        )])
    })
}

fn build_google_tool_call_id(name: &str, index: usize) -> String { format!("google::{name}::{index}") }

fn extract_google_function_name(tool_call_id: Option<&str>) -> String {
    tool_call_id
        .and_then(|value| value.split("::").nth(1))
        .unwrap_or("tool")
        .to_string()
}

pub(crate) fn google_stream_url(base_url: &str, model: &str, stream: bool) -> String {
    let trimmed = base_url.trim_end_matches('/');
    let endpoint_suffix = if stream { ":streamGenerateContent?alt=sse" } else { ":generateContent" };

    if let Some(models_index) = trimmed.find("/models/") {
        let model_path = &trimmed[models_index + "/models/".len() ..];
        let model_name = model_path
            .split(':')
            .next()
            .filter(|name| !name.is_empty())
            .unwrap_or(model);
        let prefix = &trimmed[.. models_index];
        return format!("{prefix}/models/{model_name}{endpoint_suffix}");
    }

    format!("{trimmed}/models/{model}{endpoint_suffix}")
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn dummy_settings(api_type: ApiType) -> AssistantSettings {
        let mut assistant = AssistantSettings::default();
        assistant.assistant_role = Some("System role".to_string());
        assistant.api_type = api_type;
        assistant.stream = false;
        assistant.chat_model = "dummy-model".to_string();
        assistant.advertisement = false;
        assistant.tools = Some(true);
        assistant
    }

    #[test]
    fn test_build_conversation_preserves_order() {
        let settings = dummy_settings(ApiType::OpenAiResponses);
        let request = build_conversation(
            &settings,
            vec![],
            vec![
                SublimeInputContent {
                    content: Some("selection".to_string()),
                    path: None,
                    scope: None,
                    input_kind: InputKind::ViewSelection,
                    tool_id: None,
                },
                SublimeInputContent {
                    content: Some("command".to_string()),
                    path: None,
                    scope: None,
                    input_kind: InputKind::Command,
                    tool_id: None,
                },
            ],
        );

        assert_eq!(request.messages.len(), 2);
        assert_eq!(request.messages[0].content, "selection");
        assert_eq!(request.messages[1].content, "command");
    }

    #[test]
    fn test_google_stream_url_generation() {
        assert_eq!(
            google_stream_url(
                "https://generativelanguage.googleapis.com/v1beta",
                "gemini-2.5-flash",
                false
            ),
            "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash:generateContent"
        );
        assert_eq!(
            google_stream_url("https://generativelanguage.googleapis.com/v1beta", "gemini-2.5-flash", true),
            "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash:streamGenerateContent?alt=sse"
        );
    }

    #[test]
    fn test_google_tool_id_roundtrip() {
        let id = build_google_tool_call_id("read_region_content", 2);
        assert_eq!(
            extract_google_function_name(Some(&id)),
            "read_region_content"
        );
    }

    #[test]
    fn test_prepare_openai_responses_payload() {
        let settings = dummy_settings(ApiType::OpenAiResponses);
        let payload = prepare_payload(
            &settings,
            vec![],
            vec![SublimeInputContent {
                content: Some("hello".to_string()),
                path: None,
                scope: None,
                input_kind: InputKind::Command,
                tool_id: None,
            }],
        )
        .unwrap();

        let payload_json: Value = serde_json::from_str(&payload).unwrap();
        assert_eq!(
            payload_json
                .get("model")
                .and_then(Value::as_str),
            Some("dummy-model")
        );
        assert!(
            payload_json
                .get("instructions")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .contains("System role")
        );
        assert_eq!(
            payload_json["input"][0]["type"],
            "message"
        );
        assert_eq!(payload_json["input"][0]["role"], "user");
        assert_eq!(
            payload_json["input"][0]["content"][0]["text"],
            "hello"
        );
    }

    #[test]
    fn test_prepare_anthropic_payload_with_tool_result() {
        let settings = dummy_settings(ApiType::Anthropic);
        let payload = prepare_payload(
            &settings,
            vec![],
            vec![SublimeInputContent {
                content: Some("{\"ok\":true}".to_string()),
                path: None,
                scope: None,
                input_kind: InputKind::FunctionResult,
                tool_id: Some("call_123".to_string()),
            }],
        )
        .unwrap();

        let payload_json: Value = serde_json::from_str(&payload).unwrap();
        assert!(
            payload_json["system"]
                .as_str()
                .unwrap_or_default()
                .contains("System role")
        );
        assert_eq!(
            payload_json["messages"][0]["role"],
            "user"
        );
        assert_eq!(
            payload_json["messages"][0]["content"][0]["type"],
            "tool_result"
        );
        assert_eq!(
            payload_json["messages"][0]["content"][0]["tool_use_id"],
            "call_123"
        );
    }

    #[test]
    fn test_prepare_google_payload_with_system_instruction() {
        let settings = dummy_settings(ApiType::Google);
        let payload = prepare_payload(
            &settings,
            vec![],
            vec![SublimeInputContent {
                content: Some("ping".to_string()),
                path: None,
                scope: None,
                input_kind: InputKind::ViewSelection,
                tool_id: None,
            }],
        )
        .unwrap();

        let payload_json: Value = serde_json::from_str(&payload).unwrap();
        assert!(
            payload_json["systemInstruction"]["parts"][0]["text"]
                .as_str()
                .unwrap_or_default()
                .contains("System role")
        );
        assert!(
            payload_json
                .get("system_instruction")
                .is_none()
        );
        assert!(
            payload_json
                .get("generationConfig")
                .is_some()
        );
        assert!(
            payload_json
                .get("generation_config")
                .is_none()
        );
        assert_eq!(
            payload_json["contents"][0]["role"],
            "user"
        );
        assert_eq!(
            payload_json["contents"][0]["parts"][0]["text"],
            "ping"
        );
    }

    #[test]
    fn test_prepare_openai_compat_payload_normalizes_tool_schema() {
        let settings = dummy_settings(ApiType::OpenAi);
        let payload = prepare_payload(&settings, vec![], vec![]).unwrap();
        let payload_json: Value = serde_json::from_str(&payload).unwrap();
        let tools = payload_json["tools"]
            .as_array()
            .expect("Expected OpenAI-compatible tools array");

        let read_region = tools
            .iter()
            .find(|tool| tool["function"]["name"] == "read_region_content")
            .expect("Expected read_region_content tool");
        assert!(read_region["function"].get("strict").is_none());
        assert!(
            read_region["function"]["parameters"]
                .get("additionalProperties")
                .is_none()
        );
        assert!(
            read_region["function"]["parameters"]["properties"]["region"]
                .get("additionalProperties")
                .is_none()
        );

        let get_dir = tools
            .iter()
            .find(|tool| tool["function"]["name"] == "get_working_directory_content")
            .expect("Expected get_working_directory_content tool");
        assert!(
            get_dir["function"]["parameters"]["properties"]["respect_gitignore"]
                .get("default")
                .is_none()
        );
    }

    #[test]
    fn test_prepare_google_payload_uses_camel_case_tool_fields() {
        let settings = dummy_settings(ApiType::Google);
        let payload = prepare_payload(
            &settings,
            vec![CacheEntry {
                content: Some("Calling tool".to_string()),
                thinking: None,
                path: None,
                scope: None,
                role: Roles::Assistant,
                tool_calls: Some(vec![ToolCall {
                    id: "call_123".to_string(),
                    r#type: "function".to_string(),
                    thought_signature: Some("sig_123".to_string()),
                    function: Function {
                        name: "read_region_content".to_string(),
                        arguments: "{\"file_path\":\"src/lib.rs\"}".to_string(),
                    },
                }]),
                tool_call_id: None,
                provider_metadata: None,
            }],
            vec![SublimeInputContent {
                content: Some("{\"ok\":true}".to_string()),
                path: None,
                scope: None,
                input_kind: InputKind::FunctionResult,
                tool_id: Some("call_123".to_string()),
            }],
        )
        .unwrap();

        let payload_json: Value = serde_json::from_str(&payload).unwrap();
        assert!(
            payload_json["tools"][0]
                .get("functionDeclarations")
                .is_some()
        );
        assert!(
            payload_json["tools"][0]
                .get("function_declarations")
                .is_none()
        );
        let declarations = payload_json["tools"][0]["functionDeclarations"]
            .as_array()
            .expect("Expected functionDeclarations array");
        assert!(
            declarations
                .iter()
                .any(|declaration| declaration["name"] == "read_region_content")
        );
        assert!(
            payload_json["contents"][0]["parts"][1]
                .get("functionCall")
                .is_some()
        );
        assert!(
            payload_json["contents"][0]["parts"][1]
                .get("function_call")
                .is_none()
        );
        assert_eq!(
            payload_json["contents"][0]["parts"][1]["functionCall"]["args"]["file_path"],
            "src/lib.rs"
        );
        assert_eq!(
            payload_json["contents"][0]["parts"][1]["thoughtSignature"],
            "sig_123"
        );
        assert!(
            payload_json["contents"][0]["parts"][1]["functionCall"]
                .get("thoughtSignature")
                .is_none()
        );
        assert!(
            payload_json["contents"][1]["parts"][0]
                .get("functionResponse")
                .is_some()
        );
        assert!(
            payload_json["contents"][1]["parts"][0]
                .get("function_response")
                .is_none()
        );
        assert_eq!(
            payload_json["contents"][1]["parts"][0]["functionResponse"]["response"]["ok"],
            true
        );
    }

    #[test]
    fn test_prepare_google_payload_normalizes_tool_schema_for_gemini() {
        let settings = dummy_settings(ApiType::Google);
        let payload = prepare_payload(&settings, vec![], vec![]).unwrap();
        let payload_json: Value = serde_json::from_str(&payload).unwrap();
        let declarations = payload_json["tools"][0]["functionDeclarations"]
            .as_array()
            .expect("Expected functionDeclarations array");

        let read_region = declarations
            .iter()
            .find(|declaration| declaration["name"] == "read_region_content")
            .expect("Expected read_region_content declaration");

        assert!(
            read_region["parameters"]
                .get("additionalProperties")
                .is_none()
        );
        assert!(
            read_region["parameters"]["properties"]["region"]
                .get("additionalProperties")
                .is_none()
        );

        let get_dir = declarations
            .iter()
            .find(|declaration| declaration["name"] == "get_working_directory_content")
            .expect("Expected get_working_directory_content declaration");

        assert!(
            get_dir["parameters"]["properties"]["respect_gitignore"]
                .get("default")
                .is_none()
        );
    }

    #[test]
    fn test_parse_google_response_into_tool_call() {
        let response: GoogleGenerateContentResponse = serde_json::from_value(json!({
            "candidates": [{
                "content": {
                    "role": "model",
                    "parts": [{
                        "thoughtSignature": "sig_123",
                        "functionCall": {
                            "name": "read_region_content",
                            "args": { "file_path": "src/lib.rs" }
                        }
                    }]
                }
            }]
        }))
        .unwrap();

        let message = response.into_assistant_message();
        assert_eq!(
            message
                .tool_calls
                .as_ref()
                .unwrap()[0]
                .function
                .name,
            "read_region_content"
        );
        assert_eq!(
            message
                .tool_calls
                .as_ref()
                .unwrap()[0]
                .thought_signature
                .as_deref(),
            Some("sig_123")
        );
    }

    #[test]
    fn test_prepare_google_payload_preserves_exact_part_order_and_duplicate_tool_responses() {
        let settings = dummy_settings(ApiType::Google);
        let payload = prepare_payload(
            &settings,
            vec![
                CacheEntry {
                    content: Some("flattened fallback".to_string()),
                    thinking: None,
                    path: None,
                    scope: None,
                    role: Roles::Assistant,
                    tool_calls: Some(vec![
                        ToolCall {
                            id: "google::read_region_content::1".to_string(),
                            r#type: "function".to_string(),
                            thought_signature: Some("sig_1".to_string()),
                            function: Function {
                                name: "read_region_content".to_string(),
                                arguments: "{\"region_id\":\"one\"}".to_string(),
                            },
                        },
                        ToolCall {
                            id: "google::read_region_content::3".to_string(),
                            r#type: "function".to_string(),
                            thought_signature: Some("sig_2".to_string()),
                            function: Function {
                                name: "read_region_content".to_string(),
                                arguments: "{\"region_id\":\"two\"}".to_string(),
                            },
                        },
                    ]),
                    tool_call_id: None,
                    provider_metadata: Some(ProviderMetadata::Google {
                        parts: vec![
                            GoogleAssistantPart::Text {
                                text: "Before ".to_string(),
                            },
                            GoogleAssistantPart::FunctionCall {
                                tool_call_id: "google::read_region_content::1".to_string(),
                                name: "read_region_content".to_string(),
                                arguments: "{\"region_id\":\"one\"}".to_string(),
                                thought_signature: Some("sig_1".to_string()),
                            },
                            GoogleAssistantPart::Text {
                                text: "Between ".to_string(),
                            },
                            GoogleAssistantPart::FunctionCall {
                                tool_call_id: "google::read_region_content::3".to_string(),
                                name: "read_region_content".to_string(),
                                arguments: "{\"region_id\":\"two\"}".to_string(),
                                thought_signature: Some("sig_2".to_string()),
                            },
                            GoogleAssistantPart::Text {
                                text: "After".to_string(),
                            },
                        ],
                    }),
                },
                CacheEntry {
                    content: Some("{\"content\":\"one\"}".to_string()),
                    thinking: None,
                    path: None,
                    scope: None,
                    role: Roles::Tool,
                    tool_calls: None,
                    tool_call_id: Some("google::read_region_content::1".to_string()),
                    provider_metadata: None,
                },
                CacheEntry {
                    content: Some("{\"content\":\"two\"}".to_string()),
                    thinking: None,
                    path: None,
                    scope: None,
                    role: Roles::Tool,
                    tool_calls: None,
                    tool_call_id: Some("google::read_region_content::3".to_string()),
                    provider_metadata: None,
                },
            ],
            vec![],
        )
        .unwrap();

        let payload_json: Value = serde_json::from_str(&payload).unwrap();
        let assistant_parts = payload_json["contents"][0]["parts"]
            .as_array()
            .expect("Expected assistant parts");
        assert_eq!(assistant_parts[0]["text"], "Before ");
        assert_eq!(
            assistant_parts[1]["functionCall"]["args"]["region_id"],
            "one"
        );
        assert_eq!(
            assistant_parts[1]["thoughtSignature"],
            "sig_1"
        );
        assert_eq!(assistant_parts[2]["text"], "Between ");
        assert_eq!(
            assistant_parts[3]["functionCall"]["args"]["region_id"],
            "two"
        );
        assert_eq!(
            assistant_parts[3]["thoughtSignature"],
            "sig_2"
        );
        assert_eq!(assistant_parts[4]["text"], "After");

        let response_parts = payload_json["contents"][1]["parts"]
            .as_array()
            .expect("Expected grouped function responses");
        assert_eq!(response_parts.len(), 2);
        assert_eq!(
            response_parts[0]["functionResponse"]["name"],
            "read_region_content"
        );
        assert_eq!(
            response_parts[0]["functionResponse"]["response"]["content"],
            "one"
        );
        assert_eq!(
            response_parts[1]["functionResponse"]["name"],
            "read_region_content"
        );
        assert_eq!(
            response_parts[1]["functionResponse"]["response"]["content"],
            "two"
        );
    }
}
