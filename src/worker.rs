use std::{
    error::Error,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use tokio::sync::mpsc;

use crate::{
    cacher::Cacher,
    network_client::NetworkClient,
    openai_network_types::{OpenAIResponse, Roles},
    stream_handler::StreamHandler,
    types::{AssistantSettings, CacheEntry, PromptMode, SublimeInputContent},
};

#[allow(unused, dead_code)]
#[derive(Clone, Debug)]
pub struct OpenAIWorker {
    // TODO: Think on is their necessity to be accessiable through whole object life?
    pub(crate) view_id: Option<usize>,
    pub(crate) window_id: usize,
    pub(crate) prompt_mode: Option<PromptMode>,
    pub(crate) contents: Vec<SublimeInputContent>,
    pub(crate) assistant_settings: Option<AssistantSettings>,
    pub(crate) proxy: Option<String>,
    pub(crate) cacher_path: String,

    cancel_signal: Arc<AtomicBool>,
}

impl OpenAIWorker {
    pub fn new(window_id: usize, path: String, proxy: Option<String>) -> Self {
        Self {
            window_id,
            view_id: None,
            prompt_mode: None,
            contents: vec![],
            assistant_settings: None,
            proxy,
            cacher_path: path,
            cancel_signal: Arc::new(AtomicBool::new(false)),
        }
    }

    pub async fn run<F>(
        &mut self,
        view_id: usize,
        contents: Vec<SublimeInputContent>,
        prompt_mode: PromptMode,
        assistant_settings: AssistantSettings,
        handler: Option<F>,
    ) -> Result<(), Box<dyn Error>>
    where
        F: FnMut(String) + Send + 'static,
    {
        // Update instance variables
        self.view_id = Some(view_id);
        self.prompt_mode = Some(prompt_mode);
        self.assistant_settings = Some(assistant_settings.clone());
        let cacher = Cacher::new(&self.cacher_path, Some("name"));
        let provider = NetworkClient::new(self.proxy.clone());

        let (tx, rx) = if assistant_settings.stream {
            let (tx, rx) = mpsc::channel(32);
            (Some(tx), Some(rx))
        } else {
            (None, None)
        };

        self.contents = contents;

        // Read from cache and extend with new contents
        let mut cache_entries: Vec<CacheEntry> = cacher.read_entries()?;
        cache_entries.extend(
            self.contents
                .iter()
                .map(|content| {
                    CacheEntry {
                        content: content.content.clone(),
                        path: content.path.clone(),
                        scope: content.scope.clone(),
                        role: Roles::User,
                        tool_call: None,
                        tool_call_id: None,
                    }
                })
                .collect::<Vec<_>>(),
        );
        for entry in &self.contents {
            cacher.write_entry(&CacheEntry::from(entry.clone()));
        }

        let payload = provider
            .prepare_payload(
                assistant_settings.clone(),
                cache_entries,
                self.contents.clone(),
            )
            .map_err(|e| format!("Failed to prepare payload: {}", e))?;

        let request = provider
            .prepare_request(assistant_settings.clone(), payload)
            .map_err(|e| format!("Failed to prepare request: {}", e))?;

        let cloned_cancel_flag = Arc::clone(&self.cancel_signal);

        // TODO: To make type to cast conditional to support various of protocols
        let execute_response = provider
            .execute_request::<OpenAIResponse>(request, tx, cloned_cancel_flag)
            .await;

        match execute_response {
            Ok(response) => {
                if let Some(rx) = rx {
                    let handler = handler.unwrap();
                    let mut stream_handler = StreamHandler::new(rx);
                    stream_handler
                        .handle_stream_with(handler)
                        .await;
                }

                let message = response
                    .choices
                    .first()
                    .cloned()
                    .ok_or(std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        "No choices found in the response",
                    ))?
                    .message;
                cacher.write_entry(&CacheEntry::from(message));
                Ok(())
            }
            Err(e) => {
                Err(format!(
                    "Failed to execute network request: {}",
                    e
                )
                .into())
            }
        }
    }

    pub fn cancel(&self) {
        self.cancel_signal
            .store(true, Ordering::SeqCst);
    }
}
