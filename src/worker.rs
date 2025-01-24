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
    pub(crate) cacher_path: String,

    cacher: Arc<Cacher>,
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
            cacher_path: path.clone(),
            cacher: Arc::new(Cacher::new(&path.as_str())),
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
        self.prompt_mode = Some(prompt_mode.clone());
        self.assistant_settings = Some(assistant_settings.clone());

        let provider = NetworkClient::new(self.proxy.clone());

        let (tx, rx) = mpsc::channel(view_id);

        let store = match prompt_mode {
            PromptMode::View => true,
            PromptMode::Phantom => false,
        };

        let result = tokio::spawn(LlmRunner::execute(
            provider,
            Arc::clone(&self.cacher),
            contents,
            assistant_settings,
            tx,
            Arc::clone(&self.cancel_signal),
            store,
        ));

        let _ = tokio::spawn(StreamHandler::handle_stream_with(
            rx, handler,
        ))
        .await;

        result.await.unwrap()
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
