use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use anyhow::Result;
use eventsource_stream::Eventsource;
use futures_util::StreamExt;
use log::debug;
use reqwest::{
    Client,
    Proxy,
    Request,
    header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue},
};
use serde_json::{Map, Value};
use tokio::{
    sync::{Mutex, mpsc::Sender},
    time::timeout,
};

use crate::{
    openai_network_types::{
        AssistantMessage,
        ErrorResponse,
        OpenAIErrorContainer,
        OpenAIResponse,
        OtherErrorContainer,
        ToolCall,
    },
    provider::{
        AnthropicResponse,
        AnthropicStreamState,
        GoogleGenerateContentResponse,
        GoogleStreamState,
        OpenAiResponsesResponse,
        OpenAiResponsesStreamState,
        google_stream_url,
        prepare_payload as prepare_provider_payload,
    },
    types::{AssistantSettings, CacheEntry, SublimeInputContent},
};

#[derive(Clone)]
pub struct NetworkClient {
    client: Client,
    headers: HeaderMap,
    timeout: usize,
}

#[derive(Default)]
struct AnthropicStreamTracker {
    block_to_tool_call: HashMap<usize, usize>,
}

#[derive(Default)]
struct OpenAiResponsesStreamTracker {
    tool_call_by_item_id: HashMap<String, usize>,
    tool_call_by_call_id: HashMap<String, usize>,
}

impl NetworkClient {
    pub(crate) fn new(proxy: Option<String>, timeout: usize) -> Self {
        let mut headers = HeaderMap::new();
        headers.insert(
            CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        headers.insert(
            ACCEPT,
            HeaderValue::from_static("application/json"),
        );

        let client = proxy
            .and_then(|proxy_line| Proxy::all(proxy_line).ok())
            .map(|proxy| {
                Client::builder()
                    .proxy(proxy)
                    .build()
                    .unwrap_or_default()
            })
            .unwrap_or_default();

        Self {
            client,
            headers,
            timeout,
        }
    }

    pub(crate) fn prepare_payload(
        &self,
        settings: AssistantSettings,
        cache_entries: Vec<CacheEntry>,
        sublime_inputs: Vec<SublimeInputContent>,
    ) -> Result<String> {
        prepare_provider_payload(&settings, cache_entries, sublime_inputs)
    }

    pub(crate) fn prepare_request(
        &self,
        settings: AssistantSettings,
        json_payload: String,
    ) -> Result<Request> {
        let url = match settings.api_type {
            crate::types::ApiType::Google => {
                google_stream_url(
                    &settings.url,
                    &settings.chat_model,
                    settings.stream,
                )
            }
            _ => settings.url.clone(),
        };
        let mut headers = self.headers.clone();
        if let Some(token) = settings.token {
            match settings.api_type {
                crate::types::ApiType::Anthropic => {
                    headers.insert(
                        "x-api-key",
                        HeaderValue::from_str(&token)?,
                    );
                    headers.insert(
                        "anthropic-version",
                        HeaderValue::from_static("2023-06-01"),
                    );
                }
                crate::types::ApiType::Google => {
                    headers.insert(
                        "x-goog-api-key",
                        HeaderValue::from_str(&token)?,
                    );
                }
                _ => {
                    let auth_header = format!("Bearer {}", token);
                    let auth_header = HeaderValue::from_str(&auth_header)?;
                    headers.insert(AUTHORIZATION, auth_header);
                }
            }
        }
        if settings.stream {
            headers.insert(
                ACCEPT,
                HeaderValue::from_static("text/event-stream"),
            );
        }

        Ok(self
            .client
            .post(url)
            .headers(headers)
            .body(json_payload)
            .build()?)
    }

    pub async fn execute_request(
        &self,
        settings: AssistantSettings,
        request: Request,
        sender: Arc<Mutex<Sender<String>>>,
        cancel_flag: Arc<AtomicBool>,
    ) -> Result<AssistantMessage> {
        let response = self
            .client
            .execute(request)
            .await?;

        #[cfg(debug_assertions)]
        use crate::logger;
        #[cfg(debug_assertions)]
        let _ = logger::setup_logger("/tmp/rsvr_log.log");

        if settings.stream {
            if response.status().is_success() {
                let mut stream = response
                    .bytes_stream()
                    .eventsource();
                let mut openai_stream_json = serde_json::json!({});
                let mut openai_stream_buffer = String::new();
                let mut responses_stream_state = OpenAiResponsesStreamState::default();
                let mut responses_stream_tracker = OpenAiResponsesStreamTracker::default();
                let mut anthropic_stream_state = AnthropicStreamState::default();
                let mut anthropic_stream_tracker = AnthropicStreamTracker::default();
                let mut google_stream_state = GoogleStreamState::default();
                let mut final_message: Option<AssistantMessage> = None;

                loop {
                    match timeout(
                        Duration::from_secs(self.timeout as u64),
                        stream.next(),
                    )
                    .await
                    {
                        Ok(Some(Ok(event))) => {
                            debug!(
                                "received event: {:?} {:?}",
                                event.event, event.data
                            );

                            if event.data.contains("[DONE]") || cancel_flag.load(Ordering::SeqCst) {
                                break;
                            }

                            match settings.api_type {
                                crate::types::ApiType::OpenAi | crate::types::ApiType::PlainText => {
                                    for json_value in Self::decode_legacy_openai_stream_values(
                                        &mut openai_stream_buffer,
                                        &event.data,
                                    ) {
                                        Self::handle_openai_stream_json(
                                            &mut openai_stream_json,
                                            &json_value,
                                            Arc::clone(&sender),
                                        )
                                        .await?;
                                    }
                                }
                                crate::types::ApiType::OpenAiResponses => {
                                    let json_value = match serde_json::from_str::<Value>(&event.data) {
                                        Ok(json) => json,
                                        Err(_) => continue,
                                    };
                                    final_message = Self::handle_responses_stream_event(
                                        &mut responses_stream_state,
                                        &mut responses_stream_tracker,
                                        &json_value,
                                        Arc::clone(&sender),
                                    )
                                    .await?;
                                }
                                crate::types::ApiType::Anthropic => {
                                    let json_value = match serde_json::from_str::<Value>(&event.data) {
                                        Ok(json) => json,
                                        Err(_) => continue,
                                    };
                                    final_message = Self::handle_anthropic_stream_event(
                                        &mut anthropic_stream_state,
                                        &mut anthropic_stream_tracker,
                                        &event.event,
                                        &json_value,
                                        Arc::clone(&sender),
                                    )
                                    .await?;
                                }
                                crate::types::ApiType::Google => {
                                    let json_value = match serde_json::from_str::<Value>(&event.data) {
                                        Ok(json) => json,
                                        Err(_) => continue,
                                    };
                                    final_message = Self::handle_google_stream_event(
                                        &mut google_stream_state,
                                        &json_value,
                                        Arc::clone(&sender),
                                    )
                                    .await?;
                                }
                            }
                        }
                        Ok(Some(Err(e))) => {
                            debug!("Error of accessing event: {:?}", e);
                            break;
                        }
                        Ok(None) => {
                            // Stream is exhausted
                            debug!("Stream is exhausted");
                            break;
                        }
                        Err(_) => {
                            // Timeout exceeded
                            debug!("Stream is stalled");
                            let cloned_sender = Arc::clone(&sender);

                            cloned_sender
                                .lock()
                                .await
                                .send("\n[STALLED]".to_string())
                                .await
                                .ok();
                            break; // fuckers from together can stall stream for more than 10 secs for R1
                        }
                    }
                }

                if cancel_flag.load(Ordering::SeqCst) {
                    let cloned_sender = Arc::clone(&sender);

                    cloned_sender
                        .lock()
                        .await
                        .send("\n[ABORTED]".to_string())
                        .await
                        .ok();
                }

                drop(sender);

                Ok(final_message.unwrap_or_else(|| {
                    match settings.api_type {
                        crate::types::ApiType::OpenAi | crate::types::ApiType::PlainText => {
                            serde_json::from_value::<OpenAIResponse>(openai_stream_json)
                                .map(|response| {
                                    response
                                        .choices
                                        .into_iter()
                                        .next()
                                })
                                .ok()
                                .flatten()
                                .map(|choice| choice.message)
                                .unwrap_or(AssistantMessage {
                                    role: crate::openai_network_types::Roles::Assistant,
                                    content: None,
                                    tool_calls: None,
                                    provider_metadata: None,
                                })
                        }
                        crate::types::ApiType::OpenAiResponses => {
                            responses_stream_state.into_assistant_message()
                        }
                        crate::types::ApiType::Anthropic => anthropic_stream_state.into_assistant_message(),
                        crate::types::ApiType::Google => google_stream_state.into_assistant_message(),
                    }
                }))
            } else {
                let status = &response.status();
                let error_body_string = response.text().await?;
                let error_object: ErrorResponse =
                    serde_json::from_str::<OpenAIErrorContainer>(&error_body_string)
                        .map(ErrorResponse::OpenAI)
                        .or_else(|_| {
                            serde_json::from_str::<OtherErrorContainer>(&error_body_string)
                                .map(ErrorResponse::Other)
                        })
                        .unwrap_or(ErrorResponse::Message(
                            error_body_string,
                        ));

                Err(anyhow::anyhow!(format!(
                    "Request failed with status: {}, the error: {}",
                    status,
                    error_object.message()
                )))
            }
        } else if response.status().is_success() {
            let json_body = response
                .json::<Value>()
                .await?;

            let message = self.parse_non_streaming_message(&settings, json_body)?;

            if let Some(content) = message.content.clone() {
                sender
                    .lock()
                    .await
                    .send(content)
                    .await
                    .ok();
            }

            Ok(message)
        } else {
            Err(anyhow::anyhow!(format!(
                "Request failed with status: {}",
                response.status()
            )))
        }
    }

    fn parse_non_streaming_message(
        &self,
        settings: &AssistantSettings,
        json_value: Value,
    ) -> Result<AssistantMessage> {
        match settings.api_type {
            crate::types::ApiType::OpenAi | crate::types::ApiType::PlainText => {
                let response = serde_json::from_value::<OpenAIResponse>(json_value)?;
                response
                    .choices
                    .into_iter()
                    .next()
                    .map(|choice| choice.message)
                    .ok_or_else(|| anyhow::anyhow!("Empty choices in response"))
            }
            crate::types::ApiType::OpenAiResponses => {
                Ok(serde_json::from_value::<OpenAiResponsesResponse>(json_value)?.into_assistant_message())
            }
            crate::types::ApiType::Anthropic => {
                Ok(serde_json::from_value::<AnthropicResponse>(json_value)?.into_assistant_message())
            }
            crate::types::ApiType::Google => {
                Ok(
                    serde_json::from_value::<GoogleGenerateContentResponse>(json_value)?
                        .into_assistant_message(),
                )
            }
        }
    }

    async fn handle_openai_stream_json(
        composable_response: &mut serde_json::Value,
        json_value: &serde_json::Value,
        sender: Arc<Mutex<Sender<String>>>,
    ) -> Result<()> {
        debug!("handle_json: {:?}", json_value);

        let _ = Self::merge_json(composable_response, json_value);

        if let Some(content) = json_value
            .get("choices")
            .and_then(|c| c.as_array())
            .and_then(|arr| arr.first())
            .and_then(|first| first.as_object())
            .and_then(Self::obtain_delta)
        {
            debug!("send_json: {:?}", content);
            sender
                .lock()
                .await
                .send(content)
                .await
                .map_err(|e| {
                    anyhow::anyhow!(format!(
                        "Failed to send the data: {}",
                        e
                    ))
                })
        } else {
            Ok(())
        }
    }

    async fn handle_responses_stream_event(
        state: &mut OpenAiResponsesStreamState,
        tracker: &mut OpenAiResponsesStreamTracker,
        json_value: &Value,
        sender: Arc<Mutex<Sender<String>>>,
    ) -> Result<Option<AssistantMessage>> {
        let event_type = json_value
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("");

        match event_type {
            "response.output_text.delta" => {
                if let Some(delta) = json_value
                    .get("delta")
                    .and_then(Value::as_str)
                {
                    state.text.push_str(delta);
                    sender
                        .lock()
                        .await
                        .send(delta.to_string())
                        .await
                        .ok();
                }
                Ok(None)
            }
            "response.output_item.added" => {
                if let Some(item) = json_value
                    .get("item")
                    .and_then(Value::as_object)
                {
                    if item
                        .get("type")
                        .and_then(Value::as_str)
                        == Some("function_call")
                    {
                        let item_id = item
                            .get("id")
                            .and_then(Value::as_str)
                            .map(str::to_string);
                        let call_id = item
                            .get("call_id")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string();
                        let name = item
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or("tool")
                            .to_string();
                        state
                            .tool_calls
                            .push(ToolCall {
                                id: if call_id.is_empty() {
                                    item_id
                                        .clone()
                                        .unwrap_or_default()
                                } else {
                                    call_id.clone()
                                },
                                r#type: "function".to_string(),
                                thought_signature: None,
                                function: crate::openai_network_types::Function {
                                    name: name.clone(),
                                    arguments: String::new(),
                                },
                            });
                        let tool_call_index = state.tool_calls.len() - 1;
                        if let Some(item_id) = item_id {
                            tracker
                                .tool_call_by_item_id
                                .insert(item_id, tool_call_index);
                        }
                        if !call_id.is_empty() {
                            tracker
                                .tool_call_by_call_id
                                .insert(call_id, tool_call_index);
                        }
                        sender
                            .lock()
                            .await
                            .send(format!("- {name}\n"))
                            .await
                            .ok();
                    }
                }
                Ok(None)
            }
            "response.function_call_arguments.delta" => {
                let delta = json_value
                    .get("delta")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                if let Some(tool_call_index) =
                    Self::find_responses_tool_call_index(state, tracker, json_value)
                {
                    let tool_call = &mut state.tool_calls[tool_call_index];
                    tool_call
                        .function
                        .arguments
                        .push_str(delta);
                }
                Ok(None)
            }
            "response.function_call_arguments.done" => {
                if let Some(tool_call_index) =
                    Self::find_responses_tool_call_index(state, tracker, json_value)
                {
                    let tool_call = &mut state.tool_calls[tool_call_index];
                    if let Some(arguments) = json_value
                        .get("arguments")
                        .and_then(Value::as_str)
                    {
                        tool_call.function.arguments = arguments.to_string();
                    }
                    if let Some(name) = json_value
                        .get("name")
                        .and_then(Value::as_str)
                    {
                        tool_call.function.name = name.to_string();
                    }
                    if let Some(call_id) = json_value
                        .get("call_id")
                        .and_then(Value::as_str)
                        .filter(|call_id| !call_id.is_empty())
                    {
                        tool_call.id = call_id.to_string();
                        tracker
                            .tool_call_by_call_id
                            .insert(call_id.to_string(), tool_call_index);
                    }
                }
                Ok(None)
            }
            "response.completed" => {
                if let Some(response) = json_value.get("response") {
                    let message = serde_json::from_value::<OpenAiResponsesResponse>(response.clone())?
                        .into_assistant_message();
                    Ok(Some(message))
                } else {
                    Ok(Some(
                        state
                            .clone()
                            .into_assistant_message(),
                    ))
                }
            }
            _ => Ok(None),
        }
    }

    fn find_responses_tool_call_index(
        state: &OpenAiResponsesStreamState,
        tracker: &OpenAiResponsesStreamTracker,
        json_value: &Value,
    ) -> Option<usize> {
        json_value
            .get("call_id")
            .and_then(Value::as_str)
            .and_then(|call_id| {
                tracker
                    .tool_call_by_call_id
                    .get(call_id)
                    .copied()
            })
            .or_else(|| {
                json_value
                    .get("item_id")
                    .and_then(Value::as_str)
                    .and_then(|item_id| {
                        tracker
                            .tool_call_by_item_id
                            .get(item_id)
                            .copied()
                    })
            })
            .or_else(|| {
                let response_id = json_value
                    .get("call_id")
                    .or_else(|| json_value.get("item_id"))
                    .and_then(Value::as_str)?;
                state
                    .tool_calls
                    .iter()
                    .position(|call| call.id == response_id)
            })
    }

    async fn handle_anthropic_stream_event(
        state: &mut AnthropicStreamState,
        tracker: &mut AnthropicStreamTracker,
        event_name: &str,
        json_value: &Value,
        sender: Arc<Mutex<Sender<String>>>,
    ) -> Result<Option<AssistantMessage>> {
        match event_name {
            "content_block_start" => {
                let block_index = json_value
                    .get("index")
                    .and_then(Value::as_u64)
                    .map(|index| index as usize);
                if let Some(block) = json_value
                    .get("content_block")
                    .and_then(Value::as_object)
                {
                    if block
                        .get("type")
                        .and_then(Value::as_str)
                        == Some("tool_use")
                    {
                        let id = block
                            .get("id")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string();
                        let name = block
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or("tool")
                            .to_string();
                        state
                            .tool_calls
                            .push(ToolCall {
                                id,
                                r#type: "function".to_string(),
                                thought_signature: None,
                                function: crate::openai_network_types::Function {
                                    name: name.clone(),
                                    arguments: String::new(),
                                },
                            });
                        if let Some(block_index) = block_index {
                            tracker
                                .block_to_tool_call
                                .insert(block_index, state.tool_calls.len() - 1);
                        }
                        sender
                            .lock()
                            .await
                            .send(format!("- {name}\n"))
                            .await
                            .ok();
                    }
                }
                Ok(None)
            }
            "content_block_delta" => {
                let index = json_value
                    .get("index")
                    .and_then(Value::as_u64)
                    .unwrap_or(0) as usize;
                if let Some(delta) = json_value
                    .get("delta")
                    .and_then(Value::as_object)
                {
                    match delta
                        .get("type")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                    {
                        "text_delta" => {
                            if let Some(text) = delta
                                .get("text")
                                .and_then(Value::as_str)
                            {
                                state.text.push_str(text);
                                sender
                                    .lock()
                                    .await
                                    .send(text.to_string())
                                    .await
                                    .ok();
                            }
                        }
                        "input_json_delta" => {
                            if let Some(partial) = delta
                                .get("partial_json")
                                .and_then(Value::as_str)
                            {
                                if let Some(tool_call_index) = tracker
                                    .block_to_tool_call
                                    .get(&index)
                                    .copied()
                                {
                                    if let Some(tool_call) = state
                                        .tool_calls
                                        .get_mut(tool_call_index)
                                    {
                                        tool_call
                                            .function
                                            .arguments
                                            .push_str(partial);
                                    }
                                } else if let Some(tool_call) = state.tool_calls.last_mut() {
                                    tool_call
                                        .function
                                        .arguments
                                        .push_str(partial);
                                }
                            }
                        }
                        _ => {}
                    }
                }
                Ok(None)
            }
            "message_stop" => {
                Ok(Some(
                    state
                        .clone()
                        .into_assistant_message(),
                ))
            }
            _ => Ok(None),
        }
    }

    async fn handle_google_stream_event(
        state: &mut GoogleStreamState,
        json_value: &Value,
        sender: Arc<Mutex<Sender<String>>>,
    ) -> Result<Option<AssistantMessage>> {
        let response = serde_json::from_value::<GoogleGenerateContentResponse>(json_value.clone())?;
        let message = response.into_assistant_message();

        if let Some(content) = message.content.clone() {
            if content.starts_with(&state.text) {
                let delta = &content[state.text.len() ..];
                if !delta.is_empty() {
                    sender
                        .lock()
                        .await
                        .send(delta.to_string())
                        .await
                        .ok();
                    state.text = content;
                }
            }
        }

        if let Some(tool_calls) = message.tool_calls.clone() {
            if tool_calls.len() > state.tool_calls.len() {
                for tool_call in &tool_calls[state.tool_calls.len() ..] {
                    sender
                        .lock()
                        .await
                        .send(format!(
                            "- {}\n",
                            tool_call.function.name
                        ))
                        .await
                        .ok();
                }
            }
            state.tool_calls = tool_calls;
        }

        state.provider_metadata = message
            .provider_metadata
            .clone();

        Ok(Some(AssistantMessage {
            role: crate::openai_network_types::Roles::Assistant,
            content: if state.text.is_empty() { message.content } else { Some(state.text.clone()) },
            tool_calls: if state.tool_calls.is_empty() { None } else { Some(state.tool_calls.clone()) },
            provider_metadata: state
                .provider_metadata
                .clone(),
        }))
    }

    fn decode_legacy_openai_stream_values(buffer: &mut String, fragment: &str) -> Vec<Value> {
        buffer.push_str(fragment);
        let mut values = Vec::new();

        loop {
            let trimmed_start = buffer.find(|character: char| !character.is_whitespace());

            let Some(start) = trimmed_start else {
                buffer.clear();
                break;
            };

            if start > 0 {
                buffer.drain(.. start);
            }

            let Some(end) = Self::find_complete_json_frame(buffer) else {
                break;
            };

            let candidate = buffer[.. end].to_string();
            buffer.drain(.. end);

            if let Ok(value) = serde_json::from_str::<Value>(&candidate) {
                values.push(value);
            }
        }

        values
    }

    fn find_complete_json_frame(input: &str) -> Option<usize> {
        let mut started = false;
        let mut depth = 0usize;
        let mut in_string = false;
        let mut escaping = false;

        for (index, character) in input.char_indices() {
            if !started {
                if character.is_whitespace() {
                    continue;
                }
                if matches!(character, '{' | '[') {
                    started = true;
                    depth = 1;
                } else {
                    return None;
                }
                continue;
            }

            if in_string {
                if escaping {
                    escaping = false;
                    continue;
                }
                match character {
                    '\\' => escaping = true,
                    '"' => in_string = false,
                    _ => {}
                }
                continue;
            }

            match character {
                '"' => in_string = true,
                '{' | '[' => depth += 1,
                '}' | ']' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        return Some(index + character.len_utf8());
                    }
                }
                _ => {}
            }
        }

        None
    }

    /// This function is actually handles the SSE stream from the llm
    /// There are two cases handled here so far:
    ///  - llm text answer: the `"content"` field is getting concantinated during
    ///    this call
    ///  - llm function call: the `"tool_calls"[0]."function"."arguments"` field is
    ///    getting concantinated during this call
    ///
    /// The main assumption here is that the response can never be mixed
    /// to contain both `"content"` and `"tool_calls"` in a single stream.
    fn merge_json(base: &mut Value, addition: &Value) -> Result<()> {
        match (base, addition) {
            (Value::Object(base_map), Value::Object(addition_map)) => {
                for (key, value) in addition_map {
                    match key.as_str() {
                        "content" => {
                            if value.is_null() {
                                eprintln!("Skipping null 'content' field");
                                continue;
                            }
                            if let Some(Value::String(existing_value)) = base_map.get_mut(key) {
                                if let Value::String(addition_value) = value {
                                    existing_value.push_str(addition_value);
                                }
                            }
                        }
                        "tool_calls" => {
                            if let (Some(base_array), Some(addition_array)) = (
                                base_map
                                    .get_mut(key)
                                    .and_then(|v| v.as_array_mut()),
                                value.as_array(),
                            ) {
                                let _ = Self::merge_tool_calls(base_array, addition_array.to_vec());
                            } else {
                                base_map.insert(key.to_string(), value.clone());
                            }
                        }
                        _ => {
                            let _ = Self::merge_json(
                                base_map
                                    .entry(key)
                                    .or_insert(Value::Null),
                                value,
                            );
                        }
                    }
                }
                Ok(())
            }
            (Value::Array(base_array), Value::Array(addition_array)) => {
                // Previous fallback: if arrays are non-empty, merge the first items.
                if !addition_array.is_empty() && !base_array.is_empty() {
                    let _ = Self::merge_json(&mut base_array[0], &addition_array[0]);
                }
                Ok(())
            }
            (base, addition) => {
                *base = addition.clone();
                Ok(())
            }
        }
    }

    fn merge_tool_calls(base_array: &mut Vec<Value>, addition_array: Vec<Value>) -> Result<()> {
        for addition_item in addition_array {
            if let Some(idx) = Self::legacy_tool_call_index(base_array, &addition_item) {
                if idx >= base_array.len() {
                    base_array.resize_with(idx + 1, || serde_json::json!({}));
                }
                let mut trimmed_addition = addition_item.clone();
                if let Value::Object(ref mut obj) = trimmed_addition {
                    obj.remove("index");
                }
                let _ = Self::merge_tool_call(&mut base_array[idx], &trimmed_addition);
            } else {
                base_array.push(serde_json::json!({}));
                let last_index = base_array.len() - 1;
                let mut trimmed_addition = addition_item.clone();
                if let Value::Object(ref mut obj) = trimmed_addition {
                    obj.remove("index");
                }
                let _ = Self::merge_tool_call(
                    &mut base_array[last_index],
                    &trimmed_addition,
                );
            }
        }
        Ok(())
    }

    fn legacy_tool_call_index(base_array: &[Value], addition_item: &Value) -> Option<usize> {
        if let Some(index) = addition_item
            .get("index")
            .and_then(Value::as_u64)
        {
            return Some(index as usize);
        }

        if let Some(id) = addition_item
            .get("id")
            .and_then(Value::as_str)
        {
            if let Some(existing_index) = base_array
                .iter()
                .position(|item| {
                    item.get("id")
                        .and_then(Value::as_str)
                        == Some(id)
                })
            {
                return Some(existing_index);
            }
        }

        if base_array.len() == 1 {
            return Some(0);
        }

        None
    }

    fn merge_tool_call(base_item: &mut Value, addition_item: &Value) -> Result<()> {
        let base_obj = base_item
            .as_object_mut()
            .expect("Expected base_item to be an object");

        let addition_function = addition_item
            .get("function")
            .and_then(Value::as_object);

        if let Some(addition_function) = addition_function {
            if let Some(base_function_map) = base_obj
                .entry("function".to_string())
                .or_insert_with(|| Value::Object(Map::new()))
                .as_object_mut()
            {
                if let Some(name) = addition_function
                    .get("name")
                    .cloned()
                {
                    if base_function_map
                        .get("name")
                        .is_none()
                    {
                        base_function_map.insert("name".to_string(), name);
                    }
                }

                if let Some(new_args) = addition_function
                    .get("arguments")
                    .and_then(Value::as_str)
                {
                    let entry = base_function_map
                        .entry("arguments".to_string())
                        .or_insert(Value::String(String::new()));
                    if let Value::String(existing_args) = entry {
                        existing_args.push_str(new_args);
                    }
                }
            }
        }

        for key in &["id", "type"] {
            if base_obj.get(*key).is_none() {
                if let Some(val) = addition_item.get(*key) {
                    base_obj.insert((*key).to_string(), val.clone());
                }
            }
        }

        Ok(())
    }

    /// This function extracts a plain string for streaming it into UI
    /// This is either `"content"` field (the actual answer of the llm) or
    /// a function call, where it is the `"arguments"` the one that actually
    /// streams.
    ///
    /// Thus there's low sense of showing the exact arguments of the call to a user
    /// only `"tool_calls"[0]."function"."name"` streams in the latter case here
    /// (it's a one shot).
    fn obtain_delta(map: &Map<String, Value>) -> Option<String> {
        if let Some(delta) = map.get("delta") {
            if let Some(content) = delta
                .get("content")
                .and_then(|c| c.as_str())
            {
                return Some(content.to_string());
            }
            if let Some(function_name) = delta
                .get("tool_calls")
                .and_then(|v| v.as_array())
                .and_then(|array| array.first())
                .and_then(|first_item| first_item.get("function"))
                .and_then(|function| function.get("name"))
            {
                // Prefix tool/function name with dash and newline
                return function_name
                    .as_str()
                    .map(|s| format!("- {}\n", s));
            }
        }

        if let Some(value) = map.values().next() {
            return value
                .as_object()
                .and_then(Self::obtain_delta);
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use tokio::{sync::mpsc, test};
    use wiremock::{
        MockServer,
        ResponseTemplate,
        matchers::{header, method},
    };

    use super::*;
    use crate::types::{ApiType, InputKind};

    #[test]
    async fn test_is_sync_and_send() {
        fn is_sync<T: Sync>() {}
        fn is_send<T: Send>() {}

        is_sync::<NetworkClient>();
        is_send::<NetworkClient>();
    }

    #[test]
    async fn test_prepare_payload() {
        let client = NetworkClient::new(None, 10);
        let mut settings = AssistantSettings::default();

        settings.api_type = ApiType::OpenAi;

        let cache_entries = vec![];
        let sublime_inputs = vec![SublimeInputContent {
            content: Some("content".to_string()),
            path: None,
            scope: None,
            input_kind: InputKind::ViewSelection,
            tool_id: None,
        }];

        let payload = client
            .prepare_payload(settings, cache_entries, sublime_inputs)
            .unwrap();

        let payload_json: serde_json::Value = serde_json::from_str(&payload).unwrap();
        let expected_payload = serde_json::json!({
            "messages": [
                {
                    "content": [
                        {
                            "text": "content",
                            "type": "text",
                        }
                    ],
                    "role": "user",
                }
            ],
            "stream": true,
            "model": "gpt-4o-mini",
        });

        assert_eq!(payload_json, expected_payload);
    }

    #[test]
    async fn test_prepare_request() {
        let client = NetworkClient::new(None, 10);
        let mut settings = AssistantSettings::default();
        settings.api_type = ApiType::OpenAi;
        let url = "https://models.inference.ai.azure.com/some/path".to_string();
        settings.url = url.clone();

        let cache_entries = vec![];
        let sublime_inputs = vec![SublimeInputContent {
            content: Some("content".to_string()),
            path: None,
            scope: None,
            input_kind: InputKind::ViewSelection,
            tool_id: None,
        }];

        let payload = client
            .prepare_payload(
                settings.clone(),
                cache_entries,
                sublime_inputs,
            )
            .unwrap();

        let request = client
            .prepare_request(settings.clone(), payload)
            .unwrap();

        assert_eq!(request.url().as_str(), url);
    }

    #[test]
    async fn test_prepare_request_for_anthropic_sets_required_headers() {
        let client = NetworkClient::new(None, 10);
        let mut settings = AssistantSettings::default();
        settings.api_type = ApiType::Anthropic;
        settings.url = "https://api.anthropic.com/v1/messages".to_string();
        settings.token = Some("anthropic-token".to_string());
        settings.stream = false;

        let request = client
            .prepare_request(settings, "{}".to_string())
            .unwrap();

        assert_eq!(
            request
                .headers()
                .get("x-api-key")
                .and_then(|value| value.to_str().ok()),
            Some("anthropic-token")
        );
        assert_eq!(
            request
                .headers()
                .get("anthropic-version")
                .and_then(|value| value.to_str().ok()),
            Some("2023-06-01")
        );
        assert_eq!(
            request
                .headers()
                .get(ACCEPT)
                .and_then(|value| value.to_str().ok()),
            Some("application/json")
        );
    }

    #[test]
    async fn test_prepare_streaming_request_for_anthropic_sets_sse_accept_header() {
        let client = NetworkClient::new(None, 10);
        let mut settings = AssistantSettings::default();
        settings.api_type = ApiType::Anthropic;
        settings.url = "https://api.anthropic.com/v1/messages".to_string();
        settings.token = Some("anthropic-token".to_string());
        settings.stream = true;

        let request = client
            .prepare_request(settings, "{}".to_string())
            .unwrap();

        assert_eq!(
            request
                .headers()
                .get(ACCEPT)
                .and_then(|value| value.to_str().ok()),
            Some("text/event-stream")
        );
    }

    #[test]
    async fn test_prepare_streaming_request_without_token_sets_sse_accept_header() {
        let client = NetworkClient::new(None, 10);
        let mut settings = AssistantSettings::default();
        settings.api_type = ApiType::OpenAiResponses;
        settings.url = "https://self-hosted.example/v1/responses".to_string();
        settings.token = None;
        settings.stream = true;

        let request = client
            .prepare_request(settings, "{}".to_string())
            .unwrap();

        assert_eq!(
            request
                .headers()
                .get(ACCEPT)
                .and_then(|value| value.to_str().ok()),
            Some("text/event-stream")
        );
    }

    #[test]
    async fn test_prepare_request_for_google_builds_native_endpoint() {
        let client = NetworkClient::new(None, 10);
        let mut settings = AssistantSettings::default();
        settings.api_type = ApiType::Google;
        settings.url = "https://generativelanguage.googleapis.com/v1beta".to_string();
        settings.chat_model = "gemini-2.5-flash".to_string();
        settings.token = Some("google-token".to_string());
        settings.stream = true;

        let request = client
            .prepare_request(settings, "{}".to_string())
            .unwrap();

        assert_eq!(
            request.url().as_str(),
            "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash:streamGenerateContent?alt=sse"
        );
        assert_eq!(
            request
                .headers()
                .get("x-goog-api-key")
                .and_then(|value| value.to_str().ok()),
            Some("google-token")
        );
    }

    #[tokio::test]
    async fn test_execute_response() {
        let mock_server = MockServer::start().await;
        let _mock = wiremock::Mock::given(method("POST"))
            .and(header(
                CONTENT_TYPE.as_str(),
                "application/json",
            ))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "model": "gpt-4o-mini",
                    "choices": [{
                        "index": 0,
                        "message": {
                            "role": "assistant",
                            "content": "hello"
                        }
                    }]
                })),
            )
            .mount(&mock_server)
            .await;

        let client = NetworkClient::new(None, 10);
        let mut settings = AssistantSettings::default();
        settings.url = mock_server.uri();
        settings.stream = false;

        let cache_entries = vec![];
        let sublime_inputs = vec![SublimeInputContent {
            content: Some("content".to_string()),
            path: None,
            scope: None,
            input_kind: InputKind::ViewSelection,
            tool_id: None,
        }];

        let payload = client
            .prepare_payload(
                settings.clone(),
                cache_entries,
                sublime_inputs,
            )
            .unwrap();

        let request = client
            .prepare_request(settings.clone(), payload)
            .unwrap();

        let (tx, _) = mpsc::channel(10);

        let response = client
            .execute_request(
                settings.clone(),
                request,
                Arc::new(Mutex::new(tx)),
                Arc::new(AtomicBool::new(false)),
            )
            .await;

        assert_eq!(
            response.unwrap().content,
            Some("hello".to_string())
        );
    }

    #[tokio::test]
    async fn test_tool_calls_non_streaming() {
        let mock_server = MockServer::start().await;

        let _mock = wiremock::Mock::given(method("POST"))
            .and(header(
                CONTENT_TYPE.as_str(),
                "application/json",
            ))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header(
                        CONTENT_TYPE.as_str(),
                        "application/json",
                    )
                    .set_body_json(serde_json::json!({
                        "model": "gpt-4o-mini",
                        "choices": [{
                            "index": 0,
                            "message": {
                                "role": "assistant",
                                "content": null,
                                "tool_calls": [{
                                    "id": "tool_1",
                                    "type": "function",
                                    "function": {
                                        "name": "fetch_data",
                                        "arguments": "{ \"param1\": \"value1\" }"
                                    }
                                }]
                            }
                        }]
                    })),
            )
            .mount(&mock_server)
            .await;

        let client = NetworkClient::new(None, 10);
        let mut settings = AssistantSettings::default();
        settings.url = mock_server.uri();
        settings.stream = false;

        let payload = "dummy payload";
        let request = client
            .prepare_request(settings.clone(), payload.to_string())
            .unwrap();

        let (tx, _) = mpsc::channel(10);

        let result = client
            .execute_request(
                settings.clone(),
                request,
                Arc::new(Mutex::new(tx)),
                Arc::new(AtomicBool::new(false)),
            )
            .await
            .unwrap();

        assert_eq!(
            result
                .tool_calls
                .as_ref()
                .unwrap()[0]
                .function
                .name,
            "fetch_data"
        );

        assert_eq!(
            result
                .tool_calls
                .as_ref()
                .unwrap()[0]
                .function
                .arguments,
            r#"{ "param1": "value1" }"#
        );
    }

    #[tokio::test]
    async fn test_execute_openai_responses_non_streaming() {
        let mock_server = MockServer::start().await;
        let _mock = wiremock::Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "id": "resp_123",
                    "output": [{
                        "type": "message",
                        "role": "assistant",
                        "content": [{
                            "type": "output_text",
                            "text": "responses output"
                        }]
                    }]
                })),
            )
            .mount(&mock_server)
            .await;

        let client = NetworkClient::new(None, 10);
        let mut settings = AssistantSettings::default();
        settings.api_type = ApiType::OpenAiResponses;
        settings.url = mock_server.uri();
        settings.stream = false;

        let request = client
            .prepare_request(settings.clone(), "{}".to_string())
            .unwrap();

        let (tx, _) = mpsc::channel(10);
        let response = client
            .execute_request(
                settings,
                request,
                Arc::new(Mutex::new(tx)),
                Arc::new(AtomicBool::new(false)),
            )
            .await
            .unwrap();

        assert_eq!(
            response.content,
            Some("responses output".to_string())
        );
        assert!(response.tool_calls.is_none());
    }

    #[tokio::test]
    async fn test_execute_anthropic_non_streaming_tool_use() {
        let mock_server = MockServer::start().await;
        let _mock = wiremock::Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "content": [{
                        "type": "tool_use",
                        "id": "toolu_1",
                        "name": "read_region_content",
                        "input": { "file_path": "src/lib.rs" }
                    }]
                })),
            )
            .mount(&mock_server)
            .await;

        let client = NetworkClient::new(None, 10);
        let mut settings = AssistantSettings::default();
        settings.api_type = ApiType::Anthropic;
        settings.url = mock_server.uri();
        settings.stream = false;

        let request = client
            .prepare_request(settings.clone(), "{}".to_string())
            .unwrap();

        let (tx, _) = mpsc::channel(10);
        let response = client
            .execute_request(
                settings,
                request,
                Arc::new(Mutex::new(tx)),
                Arc::new(AtomicBool::new(false)),
            )
            .await
            .unwrap();

        assert_eq!(
            response
                .tool_calls
                .as_ref()
                .unwrap()[0]
                .function
                .name,
            "read_region_content"
        );
        assert_eq!(
            response
                .tool_calls
                .as_ref()
                .unwrap()[0]
                .function
                .arguments,
            r#"{"file_path":"src/lib.rs"}"#
        );
    }

    #[tokio::test]
    async fn test_execute_google_non_streaming_function_call() {
        let mock_server = MockServer::start().await;
        let _mock = wiremock::Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "candidates": [{
                        "content": {
                            "role": "model",
                            "parts": [{
                                "functionCall": {
                                    "name": "apply_patch",
                                    "args": { "patch": "*** Begin Patch" }
                                }
                            }]
                        }
                    }]
                })),
            )
            .mount(&mock_server)
            .await;

        let client = NetworkClient::new(None, 10);
        let mut settings = AssistantSettings::default();
        settings.api_type = ApiType::Google;
        settings.url = mock_server.uri();
        settings.stream = false;
        settings.chat_model = "gemini-2.5-flash".to_string();

        let request = client
            .prepare_request(settings.clone(), "{}".to_string())
            .unwrap();

        let (tx, _) = mpsc::channel(10);
        let response = client
            .execute_request(
                settings,
                request,
                Arc::new(Mutex::new(tx)),
                Arc::new(AtomicBool::new(false)),
            )
            .await
            .unwrap();

        assert_eq!(
            response
                .tool_calls
                .as_ref()
                .unwrap()[0]
                .function
                .name,
            "apply_patch"
        );
        assert_eq!(
            response
                .tool_calls
                .as_ref()
                .unwrap()[0]
                .function
                .arguments,
            r#"{"patch":"*** Begin Patch"}"#
        );
    }

    #[tokio::test]
    async fn test_handle_anthropic_stream_event_maps_tool_deltas_by_content_block_index() {
        let mut state = AnthropicStreamState::default();
        let mut tracker = AnthropicStreamTracker::default();
        let (tx, mut rx) = mpsc::channel(10);
        let sender = Arc::new(Mutex::new(tx));

        NetworkClient::handle_anthropic_stream_event(
            &mut state,
            &mut tracker,
            "content_block_start",
            &serde_json::json!({
                "index": 0,
                "content_block": {
                    "type": "text",
                    "text": ""
                }
            }),
            Arc::clone(&sender),
        )
        .await
        .unwrap();

        NetworkClient::handle_anthropic_stream_event(
            &mut state,
            &mut tracker,
            "content_block_start",
            &serde_json::json!({
                "index": 1,
                "content_block": {
                    "type": "tool_use",
                    "id": "toolu_1",
                    "name": "fetch_data"
                }
            }),
            Arc::clone(&sender),
        )
        .await
        .unwrap();

        assert_eq!(
            rx.recv().await.as_deref(),
            Some("- fetch_data\n")
        );

        NetworkClient::handle_anthropic_stream_event(
            &mut state,
            &mut tracker,
            "content_block_delta",
            &serde_json::json!({
                "index": 1,
                "delta": {
                    "type": "input_json_delta",
                    "partial_json": "{\"path\":"
                }
            }),
            Arc::clone(&sender),
        )
        .await
        .unwrap();

        NetworkClient::handle_anthropic_stream_event(
            &mut state,
            &mut tracker,
            "content_block_delta",
            &serde_json::json!({
                "index": 1,
                "delta": {
                    "type": "input_json_delta",
                    "partial_json": "\"src\"}"
                }
            }),
            sender,
        )
        .await
        .unwrap();

        assert_eq!(state.tool_calls.len(), 1);
        assert_eq!(
            state.tool_calls[0]
                .function
                .name,
            "fetch_data"
        );
        assert_eq!(
            state.tool_calls[0]
                .function
                .arguments,
            "{\"path\":\"src\"}"
        );
    }

    #[tokio::test]
    async fn test_handle_responses_stream_event_maps_argument_deltas_by_item_id() {
        let mut state = OpenAiResponsesStreamState::default();
        let mut tracker = OpenAiResponsesStreamTracker::default();
        let (tx, mut rx) = mpsc::channel(10);
        let sender = Arc::new(Mutex::new(tx));

        NetworkClient::handle_responses_stream_event(
            &mut state,
            &mut tracker,
            &serde_json::json!({
                "type": "response.output_item.added",
                "item": {
                    "id": "item_1",
                    "type": "function_call",
                    "call_id": "call_1",
                    "name": "read_file"
                }
            }),
            Arc::clone(&sender),
        )
        .await
        .unwrap();

        assert_eq!(
            rx.recv().await.as_deref(),
            Some("- read_file\n")
        );

        NetworkClient::handle_responses_stream_event(
            &mut state,
            &mut tracker,
            &serde_json::json!({
                "type": "response.function_call_arguments.delta",
                "item_id": "item_1",
                "delta": "{\"path\":"
            }),
            Arc::clone(&sender),
        )
        .await
        .unwrap();

        NetworkClient::handle_responses_stream_event(
            &mut state,
            &mut tracker,
            &serde_json::json!({
                "type": "response.function_call_arguments.done",
                "item_id": "item_1",
                "call_id": "call_1",
                "arguments": "{\"path\":\"src/lib.rs\"}"
            }),
            sender,
        )
        .await
        .unwrap();

        assert_eq!(state.tool_calls.len(), 1);
        assert_eq!(state.tool_calls[0].id, "call_1");
        assert_eq!(
            state.tool_calls[0]
                .function
                .name,
            "read_file"
        );
        assert_eq!(
            state.tool_calls[0]
                .function
                .arguments,
            "{\"path\":\"src/lib.rs\"}"
        );
    }

    #[tokio::test]
    async fn test_handle_responses_stream_event_backfills_name_and_call_id_from_done_event() {
        let mut state = OpenAiResponsesStreamState::default();
        let mut tracker = OpenAiResponsesStreamTracker::default();
        let (tx, mut rx) = mpsc::channel(10);
        let sender = Arc::new(Mutex::new(tx));

        NetworkClient::handle_responses_stream_event(
            &mut state,
            &mut tracker,
            &serde_json::json!({
                "type": "response.output_item.added",
                "item": {
                    "id": "item_1",
                    "type": "function_call"
                }
            }),
            Arc::clone(&sender),
        )
        .await
        .unwrap();

        assert_eq!(
            rx.recv().await.as_deref(),
            Some("- tool\n")
        );

        NetworkClient::handle_responses_stream_event(
            &mut state,
            &mut tracker,
            &serde_json::json!({
                "type": "response.function_call_arguments.delta",
                "item_id": "item_1",
                "delta": "{\"path\":"
            }),
            Arc::clone(&sender),
        )
        .await
        .unwrap();

        NetworkClient::handle_responses_stream_event(
            &mut state,
            &mut tracker,
            &serde_json::json!({
                "type": "response.function_call_arguments.done",
                "item_id": "item_1",
                "call_id": "call_1",
                "name": "read_file",
                "arguments": "{\"path\":\"src/lib.rs\"}"
            }),
            sender,
        )
        .await
        .unwrap();
        assert_eq!(state.tool_calls.len(), 1);
        assert_eq!(state.tool_calls[0].id, "call_1");
        assert_eq!(
            state.tool_calls[0]
                .function
                .name,
            "read_file"
        );
        assert_eq!(
            state.tool_calls[0]
                .function
                .arguments,
            "{\"path\":\"src/lib.rs\"}"
        );
    }

    #[::core::prelude::v1::test]
    fn test_decode_legacy_openai_stream_values_reassembles_split_json_patch() {
        let mut buffer = String::new();

        let first = r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_1","type":"function","function":{"name":"read_file","arguments":"{\"path\":\"src"#;
        let second = r#"/lib.rs\"}"}}]},"finish_reason":null,"index":0}],"created":1,"id":"chatcmpl_1","model":"some_model","object":"chat.completion.chunk"}"#;

        assert!(NetworkClient::decode_legacy_openai_stream_values(&mut buffer, first).is_empty());
        let values = NetworkClient::decode_legacy_openai_stream_values(&mut buffer, second);

        assert_eq!(values.len(), 1);
        assert_eq!(
            values[0]["choices"][0]["delta"]["tool_calls"][0]["function"]["name"],
            "read_file"
        );
        assert_eq!(buffer, "");
    }

    #[::core::prelude::v1::test]
    fn test_merge_tool_call_backfills_function_name_after_arguments_arrive_first() {
        let mut base = serde_json::json!({
            "function": {
                "arguments": "{\"path\":\"src"
            }
        });

        NetworkClient::merge_tool_call(
            &mut base,
            &serde_json::json!({
                "id": "call_1",
                "type": "function",
                "function": {
                    "name": "read_file",
                    "arguments": "/lib.rs\"}"
                }
            }),
        )
        .unwrap();

        assert_eq!(base["id"], "call_1");
        assert_eq!(base["type"], "function");
        assert_eq!(base["function"]["name"], "read_file");
        assert_eq!(
            base["function"]["arguments"],
            "{\"path\":\"src/lib.rs\"}"
        );
    }

    // Cancel definitely working at the point 2700dcb298a3abcd88c62da0b5324be2d2739eb2
    // Seems like is too slow to abort the stream, it could be caused by that previously stream receiving handler
    // started working after the whole remote stream was processed beforehand.
    #[tokio::test]
    async fn test_network_client_abort() {
        let mock_server = MockServer::start().await;

        // SSE content for testing
        let sse_data = r#"
        data: {"choices":[{"delta":{"content":"The","role":"assistant","tool_calls":null},"finish_reason":null,"index":0}],"created":1734374933,"id":"cmpl-9775b1b7-0746-470e-a541-e0cc8f73bcce","model":"Llama-3.3-70B-Instruct","object":"chat.completion.chunk","usage":null}

        data: {"choices":[{"delta":{"content":" ","role":"assistant","tool_calls":null},"finish_reason":null,"index":0}],"created":1734374933,"id":"cmpl-9775b1b7-0746-470e-a541-e0cc8f73bcce","model":"Llama-3.3-70B-Instruct","object":"chat.completion.chunk","usage":null}

        data: {"choices":[{"delta":{"content":"FAIL","role":"assistant","tool_calls":null},"finish_reason":null,"index":0}],"created":1734374933,"id":"cmpl-9775b1b7-0746-470e-a541-e0cc8f73bcce","model":"Llama-3.3-70B-Instruct","object":"chat.completion.chunk","usage":null}

        data: [DONE]

        "#;

        wiremock::Mock::given(method("POST"))
            .and(header(
                CONTENT_TYPE.as_str(),
                "application/json",
            ))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header(
                        CONTENT_TYPE.as_str(),
                        "text/event-stream; charset=utf-8",
                    )
                    .set_body_string(sse_data),
            )
            .mount(&mock_server)
            .await;

        let settings = AssistantSettings {
            name: "Test Assistant".to_string(),
            output_mode: crate::types::PromptMode::Phantom,
            chat_model: "gpt-4o-mini".to_string(),
            url: mock_server.uri(),
            token: None,
            assistant_role: None,
            temperature: None,
            max_tokens: None,
            max_completion_tokens: None,
            reasoning_effort: None,
            top_p: None,
            frequency_penalty: None,
            presence_penalty: None,
            tools: None,
            parallel_tool_calls: None,
            timeout: 10,
            stream: true,
            advertisement: false,
            api_type: ApiType::OpenAi,
        };

        let cancel_flag = Arc::new(AtomicBool::new(false));

        let cancel_flag_clone = Arc::clone(&cancel_flag);

        let (tx, mut rx) = mpsc::channel(10);

        let task = tokio::spawn(async move {
            let client = NetworkClient::new(None, 10);
            let payload = "dummy payload";
            let request = client
                .prepare_request(settings.clone(), payload.to_string())
                .unwrap();

            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

            let response = client
                .execute_request(
                    settings.clone(),
                    request,
                    Arc::new(Mutex::new(tx)),
                    cancel_flag_clone,
                )
                .await;

            match response {
                Ok(_) => println!("Request completed successfully!"),
                Err(e) => println!("Request failed: {:?}", e),
            }
        });

        cancel_flag.store(true, Ordering::SeqCst);

        let mut output = vec![];
        while let Some(string) = rx.recv().await {
            output.push(string);
        }

        let _ = task.await;

        assert!(output.contains(&"\n[ABORTED]".to_string()))
    }
}
