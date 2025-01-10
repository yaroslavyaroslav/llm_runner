use serde_json::from_str;
use std::error::Error;
use tokio::sync::mpsc;

use crate::cacher::Cacher;
use crate::network_client::NetworkClient;
use crate::openai_network_types::{OpenAIResponse, Roles};
use crate::types::{AssistantSettings, CacheEntry, PromptMode, SublimeInputContent};

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
        }
    }

    pub async fn run(
        &mut self,
        view_id: usize,
        contents: String, // encoded `Vec<SublimeInputContent>`
        prompt_mode: PromptMode,
        assistant_settings: AssistantSettings,
    ) -> Result<(), Box<dyn Error>> {
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

        // Decode the contents
        self.contents =
            from_str::<Vec<SublimeInputContent>>(&contents).map_err(|e| format!("Failed to decode contents: {}", e))?;

        // Read from cache and extend with new contents
        let mut cache_entries: Vec<CacheEntry> = cacher.read_entries()?;
        cache_entries.extend(
            self.contents
                .iter()
                .map(|content| CacheEntry {
                    content: content.content.clone(),
                    path: content.path.clone(),
                    scope: content.scope.clone(),
                    role: Roles::User,
                    tool_call: None,
                    tool_call_id: None,
                })
                .collect::<Vec<_>>(),
        );
        for entry in &self.contents {
            cacher.write_entry(&CacheEntry::from(entry.clone()));
        }

        let payload = provider
            .prepare_payload(assistant_settings.clone(), cache_entries, self.contents.clone())
            .map_err(|e| format!("Failed to prepare payload: {}", e))?;

        let request = provider
            .prepare_request(assistant_settings.clone(), payload)
            .map_err(|e| format!("Failed to prepare request: {}", e))?;

        // TODO: To make type to cast conditional to support various of protocols
        let execute_response = provider
            .execute_request::<OpenAIResponse>(request, tx)
            .await;

        match execute_response {
            Ok(response) => {
                if let Some(mut rx) = rx {
                    while let Some(data) = rx.recv().await {
                        println!("Streaming data: {}", data);
                    }
                }
                println!("Response: {:?}", response);
                Ok(())
            }
            Err(e) => Err(format!("Failed to execute network request: {}", e).into()),
        }
    }
}
