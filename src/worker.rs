use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use anyhow::Result;
use tokio::sync::mpsc;

use crate::{
    cacher::Cacher,
    network_client::NetworkClient,
    runner::LlmRunner,
    stream_handler::StreamHandler,
    types::{AssistantSettings, PromptMode, SublimeInputContent},
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

    pub async fn run(
        &mut self,
        view_id: usize,
        contents: Vec<SublimeInputContent>,
        prompt_mode: PromptMode,
        assistant_settings: AssistantSettings,
        handler: Arc<dyn Fn(String) + Send + Sync + 'static>,
    ) -> Result<()> {
        self.view_id = Some(view_id);
        self.prompt_mode = Some(prompt_mode);
        self.assistant_settings = Some(assistant_settings.clone());
        let cacher = Cacher::new(
            self.cacher_path
                .as_mut()
                .map(|s| s.as_str()),
        );
        let provider = NetworkClient::new(self.proxy.clone());

        let (tx, rx) = mpsc::channel(view_id);

        let result = LlmRunner::execute(
            provider,
            &cacher,
            contents,
            assistant_settings,
            tx,
            Arc::clone(&self.cancel_signal),
        )
        .await;

        StreamHandler::handle_stream_with(rx, handler).await;

        result
    }

    pub fn cancel(&self) {
        self.cancel_signal
            .store(true, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_sync_and_send() {
        fn is_sync<T: Sync>() {}
        fn is_send<T: Send>() {}

        is_sync::<OpenAIWorker>();
        is_send::<OpenAIWorker>();
    }
}
