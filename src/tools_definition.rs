use once_cell::sync::Lazy;
use serde_json::json;

use crate::openai_network_types::{FunctionToCall, Tool};

pub static CREATE_FILE: Lazy<Tool> = Lazy::new(|| {
    Tool {
        r#type: "function".to_string(),
        function: Some(FunctionToCall {
            name: "create_file".to_string(),
            description: Some("Create a new file with the specified content at the given path.".to_string()),
            parameters: json!({
                "properties": {
                    "file_path": {
                        "type": "string",
                        "description": "The path where the file will be created."
                    }
                },
                "type": "object",
                "required": ["file_path"],
                "additionalProperties": false
            })
            .as_object()
            .cloned(),
            strict: Some(true),
        }),
    }
});
