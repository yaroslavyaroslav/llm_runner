use std::collections::HashMap;
use std::error::Error;
use std::sync::mpsc;
use std::thread;

use serde::Serialize;

use crate::cacher::Cacher;
use crate::network_client::NetworkClient;
use crate::types::AssistantSettings;

struct OpenAIWorker {
    view_id: usize,
    mode: String,
    command: Option<String>,
    assistant: AssistantSettings,
    sheets: Option<Vec<String>>,
    cacher: Cacher,
    provider: NetworkClient,
}

// impl OpenAIWorker {
//     fn new(
//         region: Option<String>,
//         selected_text: String,
//         view: String,
//         mode: String,
//         command: Option<String>,
//         assistant: AssistantSettings,
//         sheets: Option<Vec<String>>,
//     ) -> Self {
//         let cacher = Cacher::new();
//         let provider = NetworkClient::new(assistant.clone());
//         let listner = OutputPanelListener::new(true);
//         let phantom_manager = PhantomStreamer::new(view.clone(), cacher.clone());

//         Self {
//             region,
//             selected_text,
//             view,
//             mode,
//             command,
//             assistant,
//             sheets,
//             cacher,
//             provider,
//             listner,
//             phantom_manager,
//         }
//     }

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

//     fn run(&self) -> Result<(), Box<dyn Error>> {
//         // Simulate running the worker
//         Ok(())
//     }
// }

// fn main() -> Result<(), Box<dyn Error>> {
//     let (tx, rx) = mpsc::channel();

//     let worker = OpenAIWorker::new(
//         None,
//         "Selected text".to_string(),
//         "View".to_string(),
//         "Mode".to_string(),
//         Some("Command".to_string()),
//         AssistantSettings {
//             max_tokens: 100,
//             stream: true,
//             prompt_mode: "panel".to_string(),
//             assistant_role: "Assistant role".to_string(),
//         },
//         None,
//     );

//     let handle = thread::spawn(move || {
//         worker.run().unwrap();
//     });

//     handle.join().unwrap();

//     Ok(())
// }
