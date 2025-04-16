use std::sync::{atomic::AtomicBool, Arc};

use anyhow::Result;
use tokio::sync::{mpsc::Sender, Mutex};

use crate::{
    cacher::Cacher,
    network_client::NetworkClient,
    openai_network_types::{OpenAIResponse, ToolCall},
    types::{ApiType, AssistantSettings, CacheEntry, InputKind, SublimeInputContent},
};

#[allow(unused, dead_code)]
#[derive(Clone, Debug)]
pub struct LlmRunner;

impl LlmRunner {
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn execute(
        provider: NetworkClient,
        cacher: Arc<Mutex<Cacher>>,
        contents: Vec<SublimeInputContent>,
        assistant_settings: AssistantSettings,
        sender: Arc<Mutex<Sender<String>>>,
        function_handler: Arc<dyn Fn((String, String)) -> String + Send + Sync + 'static>,
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

        if assistant_settings.api_type == ApiType::PlainText {
            Self::new_response_handle(
                provider,
                cacher,
                assistant_settings,
                sender,
                function_handler,
                cancel_flag,
                store,
                request,
            )
            .await
        } else {
            Self::plain_response_handle(
                provider,
                cacher,
                assistant_settings,
                sender,
                function_handler,
                cancel_flag,
                store,
                request,
            )
            .await
        }
    }

    fn handle_function_call(
        tool_calls: Vec<ToolCall>,
        function_handler: Arc<dyn Fn((String, String)) -> String + Send + Sync + 'static>,
    ) -> Vec<SublimeInputContent> {
        tool_calls
            .iter()
            .map(|tool_call| {
                LlmRunner::pick_function(
                    tool_call.clone(),
                    Arc::clone(&function_handler),
                )
            })
            .collect::<Vec<_>>()
    }

    fn pick_function(
        tool: ToolCall,
        function_handler: Arc<dyn Fn((String, String)) -> String + Send + Sync + 'static>,
    ) -> SublimeInputContent {
        let name = tool.function.name.clone();
        let args = tool.function.arguments;
        let response = function_handler((name, args));

        SublimeInputContent {
            content: Some(response),
            input_kind: InputKind::FunctionResult,
            tool_id: Some(tool.id),
            path: None,
            scope: None,
        }
    }

    async fn plain_response_handle(
        provider: NetworkClient,
        cacher: Arc<Mutex<Cacher>>,
        assistant_settings: AssistantSettings,
        sender: Arc<Mutex<Sender<String>>>,
        _function_handler: Arc<dyn Fn((String, String)) -> String + Send + Sync>,
        cancel_flag: Arc<AtomicBool>,
        store: bool,
        request: reqwest::Request,
    ) -> std::result::Result<(), anyhow::Error> {
        // TODO: To make type to cast conditional to support various of protocols
        let result = provider
            .execute_responses_request(
                request,
                Arc::clone(&sender),
                Arc::clone(&cancel_flag),
                assistant_settings.stream,
            )
            .await;

        if store {
            cacher
                .lock()
                .await
                .write_entry(&CacheEntry::from(result.unwrap()))
        } else {
            result.map(|_| ())
        }
    }

    async fn new_response_handle(
        provider: NetworkClient,
        cacher: Arc<Mutex<Cacher>>,
        assistant_settings: AssistantSettings,
        sender: Arc<Mutex<Sender<String>>>,
        function_handler: Arc<dyn Fn((String, String)) -> String + Send + Sync>,
        cancel_flag: Arc<AtomicBool>,
        store: bool,
        request: reqwest::Request,
    ) -> std::result::Result<(), anyhow::Error> {
        // TODO: To make type to cast conditional to support various of protocols
        let result = provider
            .execute_plain_api_request::<OpenAIResponse>(
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
            if let Ok(ref message) = result {
                cacher
                    .lock()
                    .await
                    .write_entry(&CacheEntry::from(
                        message.clone().choices[0]
                            .message
                            .clone(),
                    ))
                    .ok();
            }

            let content = LlmRunner::handle_function_call(
                tool_calls,
                Arc::clone(&function_handler),
            );

            Box::pin(Self::execute(
                provider,
                Arc::clone(&cacher),
                content,
                assistant_settings,
                sender,
                function_handler,
                cancel_flag,
                true,
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
            result.map(|_| ())
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
