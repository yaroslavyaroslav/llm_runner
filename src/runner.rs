use std::{
    str::FromStr,
    sync::{atomic::AtomicBool, Arc},
};

use anyhow::Result;
use tokio::sync::{mpsc::Sender, Mutex};

use crate::{
    cacher::Cacher,
    network_client::NetworkClient,
    openai_network_types::{OpenAIResponse, ToolCall},
    tools_definition::FunctionName,
    types::{AssistantSettings, CacheEntry, InputKind, SublimeInputContent},
};

#[allow(unused, dead_code)]
#[derive(Clone, Debug)]
pub struct LlmRunner;

impl LlmRunner {
    pub(crate) async fn execute(
        provider: NetworkClient,
        cacher: Arc<Mutex<Cacher>>,
        contents: Vec<SublimeInputContent>,
        assistant_settings: AssistantSettings,
        sender: Arc<Mutex<Sender<String>>>,
        cancel_flag: Arc<AtomicBool>,
        store: bool,
    ) -> Result<()> {
        let cache_entries: Vec<CacheEntry> = cacher
            .lock()
            .await
            .read_entries()?;

        if store {
            for entry in &contents {
                if entry.input_kind != InputKind::Sheet {
                    cacher
                        .lock()
                        .await
                        .write_entry(&CacheEntry::from(entry.clone()))
                        .ok();
                }
            }
        }

        let payload = provider.prepare_payload(
            assistant_settings.clone(),
            cache_entries,
            contents.clone(),
        )?;

        let request = provider.prepare_request(assistant_settings.clone(), payload)?;

        // TODO: To make type to cast conditional to support various of protocols
        let result = provider
            .execute_request::<OpenAIResponse>(
                request,
                Arc::clone(&sender),
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
                    .lock()
                    .await
                    .write_entry(&CacheEntry::from(
                        message.choices[0]
                            .message
                            .clone(),
                    ))
                    .ok();
            }
            let content = LlmRunner::handle_function_call(tool_calls[0].clone());

            for item in content.clone() {
                cacher
                    .lock()
                    .await
                    .write_entry(&CacheEntry::from(item))
                    .ok();
            }

            Box::pin(Self::execute(
                provider,
                cacher,
                content,
                assistant_settings,
                sender,
                cancel_flag,
                true, // storing function calls chain disregarding user settings
            ))
            .await
        } else if store {
            cacher
                .lock()
                .await
                .write_entry(&CacheEntry::from(
                    result?.choices[0]
                        .message
                        .clone(),
                ))
        } else {
            Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_sync_and_send() {
        fn is_sync<T: Sync>() {}
        fn is_send<T: Send>() {}

        is_sync::<LlmRunner>();
        is_send::<LlmRunner>();
    }
}
