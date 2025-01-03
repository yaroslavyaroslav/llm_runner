use serde_json::from_str;
use std::error::Error;
// use tokio::task;

use crate::cacher::Cacher;
use crate::network_client::NetworkClient;
use crate::types::{AssistantSettings, PromptMode, SublimeInputContent};

#[allow(unused, dead_code)]
pub struct OpenAIWorker {
    // TODO: Think on is their necessary to be accessiable through whole object life?
    view_id: Option<usize>,
    window_id: usize,
    prompt_mode: Option<PromptMode>,
    contents: Vec<SublimeInputContent>,
    assistant_settings: Option<AssistantSettings>,
    cacher: Option<Cacher>,
    provider: Option<NetworkClient>,
}

impl OpenAIWorker {
    pub fn new(window_id: usize) -> Self {
        Self {
            window_id,
            view_id: None,
            prompt_mode: None,
            contents: vec![],
            assistant_settings: None,
            cacher: None,
            provider: None,
        }
    }

    //     fn handle_function_call(&self, _tool_calls: Vec<String>) -> Result<(), Box<dyn Error>> {
    //         // Simulate handling the function call
    //         Ok(())
    //     }

    //     fn handle_streaming_response(&self, _response: String) -> Result<(), Box<dyn Error>> {
    //         // Simulate handling the streaming response
    //         Ok(())
    //     }

    //     fn handle_plain_response(&self, _response: String) -> Result<(), Box<dyn Error>> {
    //         // Simulate handling the plain response
    //         Ok(())
    //     }

    //     fn handle_response(&self) -> Result<(), Box<dyn Error>> {
    //         // Simulate handling the response
    //         Ok(())
    //     }

    pub async fn run(
        &self,
        view_id: usize,
        contents: String, // encoded `Vec<SublimeInputContent>`
        prompt_mode: PromptMode,
        assistant_settings: AssistantSettings,
    ) -> Result<(), Box<dyn Error>> {
        // Initialize NetworkClient
        let network_client = NetworkClient::new();

        // // Set view_id and prompt_mode
        // let view_id = Some(view_id);
        // let prompt_mode = Some(prompt_mode);

        // // Deserializing the contents
        // let decoded_contents: Vec<SublimeInputContent> =
        //     from_str(&contents).map_err(|e| format!("Failed to decode contents: {}", e))?;

        // // Check and read cache
        // if let Some(cacher) = &self.cacher {
        //     let mut cache_entries: Vec<SublimeInputContent> = cacher.read_entries()?;
        //     cache_entries.extend(decoded_contents.clone());
        //     for entry in &decoded_contents {
        //         cacher.write_entry(entry).await;
        //     }
        // }

        // // Prepare payload and request
        // let payload = network_client
        //     .prepare_payload(assistant_settings.clone(), decoded_contents)
        //     .map_err(|e| format!("Failed to prepare payload: {}", e))?;

        // let request = network_client
        //     .prepare_request(assistant_settings.clone(), payload)
        //     .map_err(|e| format!("Failed to prepare request: {}", e))?;

        // // Execute network request
        // match network_client
        //     .execute_response::<serde_json::Value>(request, None)
        //     .await
        // {
        //     Ok(response) => println!("Response: {:?}", response),
        //     Err(e) => return Err(format!("Failed to execute network request: {}", e).into()),
        // };

        Ok(())
    }
}
