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
    runner::LlmRunner,
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
    pub(crate) cacher_path: Option<String>,

    cancel_signal: Arc<AtomicBool>,
}

impl OpenAIWorker {
    pub fn new(window_id: usize, path: Option<String>, proxy: Option<String>) -> Self {
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
        handler: F,
    ) -> Result<(), Box<dyn Error>>
    where
        F: FnMut(String) + Send + 'static,
    {
        // Update instance variables
        self.view_id = Some(view_id);
        self.prompt_mode = Some(prompt_mode);
        self.assistant_settings = Some(assistant_settings.clone());
        let cacher = Cacher::new(
            self.cacher_path
                .as_mut()
                .map(|s| s.as_str()),
        );
        let provider = NetworkClient::new(self.proxy.clone());

        let (tx, rx) = mpsc::channel(32);

        let execute_response = LlmRunner::execute(
            provider,
            &cacher,
            contents,
            assistant_settings,
            tx,
            Arc::clone(&self.cancel_signal),
        )
        .await;

        match execute_response {
            Ok(response) => {
                let handler = handler;
                let mut stream_handler = StreamHandler::new(rx);
                stream_handler
                    .handle_stream_with(handler)
                    .await;

                let message = response
                    .choices
                    .first()
                    .cloned()
                    .ok_or(std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        "No choices found in the response",
                    ))?
                    .message;

                Ok(cacher.write_entry(&CacheEntry::from(message))?)
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
