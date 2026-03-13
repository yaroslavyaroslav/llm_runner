use std::sync::{Arc, Mutex};

use serde_json::{Value, json};
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

#[derive(Clone, Debug)]
pub struct RecordedSequentialResponder {
    count: Arc<Mutex<usize>>,
    responses: Arc<Vec<ResponseTemplate>>,
    recorded_json_bodies: Arc<Mutex<Vec<Value>>>,
}

impl RecordedSequentialResponder {
    pub fn new(responses: Vec<ResponseTemplate>) -> Self {
        Self {
            count: Arc::new(Mutex::new(0)),
            responses: Arc::new(responses),
            recorded_json_bodies: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn recorded_json_bodies(&self) -> Vec<Value> {
        self.recorded_json_bodies
            .lock()
            .unwrap()
            .clone()
    }
}

impl Respond for RecordedSequentialResponder {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        if let Ok(json_body) = serde_json::from_slice::<Value>(&request.body) {
            self.recorded_json_bodies
                .lock()
                .unwrap()
                .push(json_body);
        }

        let mut count = self.count.lock().unwrap();
        let response = self
            .responses
            .get(*count)
            .cloned()
            .unwrap_or_else(|| ResponseTemplate::new(500).set_body_string("Unexpected request"));
        *count += 1;
        response
    }
}

#[derive(Clone, Debug)]
pub struct SseEvent {
    event: Option<String>,
    data: Value,
}

impl SseEvent {
    pub fn named(event: impl Into<String>, data: Value) -> Self {
        Self {
            event: Some(event.into()),
            data,
        }
    }

    pub fn data(data: Value) -> Self { Self { event: None, data } }
}

pub fn sse_response(events: Vec<SseEvent>) -> ResponseTemplate {
    let mut body = String::new();

    for event in events {
        if let Some(event_name) = event.event {
            body.push_str("event: ");
            body.push_str(&event_name);
            body.push('\n');
        }
        body.push_str("data: ");
        body.push_str(&event.data.to_string());
        body.push_str("\n\n");
    }

    body.push_str("data: [DONE]\n\n");

    ResponseTemplate::new(200)
        .insert_header("content-type", "text/event-stream")
        .set_body_string(body)
}
