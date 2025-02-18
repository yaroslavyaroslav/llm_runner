use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use anyhow::Result;
use tokio::{
    join,
    sync::{mpsc, Mutex},
};

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

    cacher: Arc<Mutex<Cacher>>,
    cancel_signal: Arc<AtomicBool>,
    pub(crate) is_alive: Arc<AtomicBool>,
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
            cacher: Arc::new(Mutex::new(Cacher::new(&path))),
            cancel_signal: Arc::new(AtomicBool::new(false)),
            is_alive: Arc::new(AtomicBool::new(false)),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn run(
        &self,
        view_id: usize,
        contents: Vec<SublimeInputContent>,
        prompt_mode: PromptMode,
        assistant_settings: AssistantSettings,
        handler: Arc<dyn Fn(String) + Send + Sync + 'static>,
        error_handler: Arc<dyn Fn(String) + Send + Sync + 'static>,
        function_handler: Arc<dyn Fn((String, String)) -> String + Send + Sync + 'static>,
    ) -> Result<()> {
        self.is_alive
            .store(true, Ordering::SeqCst);

        let provider = NetworkClient::new(
            self.proxy.clone(),
            assistant_settings.timeout,
        );

        let (tx, rx) = mpsc::channel(view_id);

        let store = match prompt_mode {
            PromptMode::View => true,
            PromptMode::Phantom => false,
        };

        let result_fut = LlmRunner::execute(
            provider,
            Arc::clone(&self.cacher),
            contents,
            assistant_settings,
            Arc::new(Mutex::new(tx)),
            Arc::clone(&function_handler),
            Arc::clone(&self.cancel_signal),
            store,
        );

        let handler_fut = StreamHandler::handle_stream_with(rx, handler);

        let (runner_result, _) = join!(result_fut, handler_fut);

        if let Err(e) = &runner_result {
            error_handler(format!("LlmRunner error: {}", e));
        }

        self.is_alive
            .store(false, Ordering::SeqCst);

        runner_result
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
