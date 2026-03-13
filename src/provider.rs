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
        system_message.push_str(
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
                    "#,
        );
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
            parameters: function
                .parameters
                .unwrap_or_default(),
        })
    }
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
