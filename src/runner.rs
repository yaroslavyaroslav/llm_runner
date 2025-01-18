use std::{
    error::Error,
    str::FromStr,
    sync::{atomic::AtomicBool, Arc},
};

use tokio::sync::mpsc::Sender;

use crate::{
    cacher::Cacher,
    network_client::NetworkClient,
    openai_network_types::{OpenAIResponse, ToolCall},
    tools_definition::FunctionName,
    types::{AssistantSettings, CacheEntry, InputKind, SublimeInputContent},
};

#[allow(unused, dead_code)]
#[derive(Clone, Debug)]
pub struct LlmRunner {}

impl LlmRunner {
    pub(crate) async fn execute(
        provider: NetworkClient,
        cacher: &Cacher,
        contents: Vec<SublimeInputContent>,
        assistant_settings: AssistantSettings,
        sender: Sender<String>,
        cancel_flag: Arc<AtomicBool>,
    ) -> Result<OpenAIResponse, Box<dyn Error>> {
        let cache_entries: Vec<CacheEntry> = cacher.read_entries()?;

        for entry in &contents {
            cacher
                .write_entry(&CacheEntry::from(entry.clone()))
                .ok();
        }

        let payload = provider
            .prepare_payload(
                assistant_settings.clone(),
                cache_entries,
                contents.clone(),
            )
            .map_err(|e| format!("Failed to prepare payload: {}", e))?;

        let request = provider
            .prepare_request(assistant_settings.clone(), payload)
            .map_err(|e| format!("Failed to prepare request: {}", e))?;

        // TODO: To make type to cast conditional to support various of protocols
        let result = provider
            .execute_request::<OpenAIResponse>(
                request,
                sender.clone(),
                Arc::clone(&cancel_flag),
                assistant_settings.stream,
            )
            .await;

        if let Some(tool_calls) = result
            .as_ref()
            .ok()
            .and_then(|r| {
                r.choices
                    .first()?
                    .message
                    .tool_calls
                    .clone()
            })
        {
            if let Ok(message) = result {
                cacher
                    .write_entry(&CacheEntry::from(
                        message.choices[0]
                            .message
                            .clone(),
                    ))
                    .ok();
            }
            let content = LlmRunner::handle_function_call(tool_calls[0].clone());

            Box::pin(Self::execute(
                provider,
                cacher,
                content,
                assistant_settings,
                sender,
                cancel_flag,
            ))
            .await
        } else {
            result
        }
    }

    fn handle_function_call(tool_call: ToolCall) -> Vec<SublimeInputContent> {
        vec![LlmRunner::pick_function(tool_call)]
    }

    fn pick_function(tool: ToolCall) -> SublimeInputContent {
        let content = match FunctionName::from_str(tool.function.name.as_str()) {
            Ok(FunctionName::CreateFile) => Some("File created".to_string()),
            Ok(FunctionName::ReadRegionContent) => {
                Some("This is test content that have been read".to_string())
            }
            Ok(FunctionName::GetWorkingDirectoryContent) => {
                Some("This will be the working directory content provided".to_string())
            }
            Ok(FunctionName::ReplaceTextWithAnotherText) => Some("Text successfully replaced".to_string()),
            Ok(FunctionName::ReplaceTextForWholeFile) => {
                Some("The whole file content successfully replaced".to_string())
            }
            Err(_) => Some("Function unknown".to_string()),
        };

        SublimeInputContent {
            content,
            input_kind: InputKind::FunctionResult,
            tool_id: Some(tool.id),
            path: None,
            scope: None,
        }
    }
}
