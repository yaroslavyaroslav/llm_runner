use std::sync::{Arc, Mutex};

use serde_json::json;
use wiremock::{Request, Respond, ResponseTemplate};

pub struct SequentialResponder {
    count: Arc<Mutex<usize>>,
}

impl SequentialResponder {
    pub fn new() -> Self {
        Self {
            count: Arc::new(Mutex::new(0)),
        }
    }
}

impl Respond for SequentialResponder {
    fn respond(&self, _: &Request) -> ResponseTemplate {
        let mut count = self.count.lock().unwrap();

        let response = match *count {
            0 => {
                ResponseTemplate::new(200).set_body_json(json!({
                    "model": "some_model",
                    "id": "some_id",
                    "created": 367123,
                    "choices": [{
                        "index": 0,
                        "message": {
                            "role": "assistant",
                            "content": null,
                            "tool_calls": [{
                                "id": "call_qZgF6u7aysFbuUdzV2z09vRW",
                                "type": "function",
                                "function": {
                                    "name": "create_file",
                                    "arguments": "{\"file_path\":\"my_new_file.txt\"}"
                                }
                            }],
                            "refusal": null
                        },
                        "logprobs": null,
                        "finish_reason": "tool_calls"
                    }]
                }))
            }
            1 => {
                ResponseTemplate::new(200).set_body_json(json!({
                    "model": "some_model",
                    "id": "some_id",
                    "created": 367123,
                    "choices": [{
                        "index": 0,
                        "message": {
                            "role": "assistant",
                            "content": "Some Content",
                            "refusal": null
                        },
                        "logprobs": null,
                        "finish_reason": "stop"
                    }]
                }))
            }
            _ => ResponseTemplate::new(500).set_body_string("Third response"),
        };

        *count += 1;
        response
    }
}
