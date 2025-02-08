use std::sync::Arc;

use once_cell::sync::Lazy;
use serde_json::json;
use strum_macros::{Display, EnumString};

use crate::openai_network_types::{FunctionToCall, Tool};

#[derive(EnumString, PartialEq, Display, Debug, Clone, Copy)]
#[strum(serialize_all = "snake_case")]
pub enum FunctionName {
    CreateFile,
    ReplaceTextWithAnotherText,
    ReplaceTextForWholeFile,
    ReadRegionContent,
    GetWorkingDirectoryContent,
}

pub static FUNCTIONS: Lazy<Vec<Arc<Tool>>> = Lazy::new(|| {
    vec![
        // Arc::new((*CREATE_FILE).clone()),
        Arc::new((*REPLACE_TEXT_FOR_WHOLE_FILE).clone()),
        Arc::new((*REPLACE_TEXT_WITH_ANOTHER_TEXT).clone()),
        Arc::new((*READ_REGION_CONTENT).clone()),
        Arc::new((*GET_WORKING_DIRECTORY_CONTENT).clone()),
    ]
});

#[allow(dead_code)]
pub static CREATE_FILE: Lazy<Tool> = Lazy::new(|| {
    Tool {
        r#type: "function".to_string(),
        function: Some(FunctionToCall {
            name: FunctionName::CreateFile.to_string(),
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

pub static REPLACE_TEXT_WITH_ANOTHER_TEXT: Lazy<Tool> = Lazy::new(|| {
    Tool {
        r#type: "function".to_string(),
        function: Some(FunctionToCall {
            name: FunctionName::ReplaceTextWithAnotherText.to_string(),
            description: Some("Replace the matched content with the new one provided".to_string()),
            parameters: json!({
                "properties": {
                    "file_path": {
                        "type": "string",
                        "description": "The path of the file where content to search is stored",
                    },
                    "old_content": {
                        "type": "string",
                        "description": "The existing content to be replaced with new content",
                    },
                    "new_content": {
                        "type": "string",
                        "description": "The content to replace the old one",
                    },
                },
                "type": "object",
                "required": ["file_path", "old_content", "new_content"],
                "additionalProperties": false,
            })
            .as_object()
            .cloned(),
            strict: Some(true),
        }),
    }
});

pub static REPLACE_TEXT_FOR_WHOLE_FILE: Lazy<Tool> = Lazy::new(|| {
    Tool {
        r#type: "function".to_string(),
        function: Some(FunctionToCall {
            name: FunctionName::ReplaceTextForWholeFile.to_string(),
            description: Some("Replace the whole text in the file with the new one".to_string()),
            parameters: json!({
                "properties": {
                    "file_path": {
                        "type": "string",
                        "description": "The path of the file where content to search is stored",
                    },
                    "create": {
                        "type": "boolean",
                        "description": "To create a new pane and file for it under a given path and with a given content. \
                        File created that way will not be visible by `get_working_directory_content` function call until user manually saves it",
                    },
                    "content": {
                        "type": "string",
                        "description": "The New content of the file",
                    },
                },
                "type": "object",
                "required": ["file_path", "create", "content"],
                "additionalProperties": false
            })
            .as_object()
            .cloned(),
            strict: Some(true),
        }),
    }
});

pub static GET_WORKING_DIRECTORY_CONTENT: Lazy<Tool> = Lazy::new(|| {
    Tool {
        r#type: "function".to_string(),
        function: Some(FunctionToCall {
            name: FunctionName::GetWorkingDirectoryContent.to_string(),
            description: Some(
                "Get complete structure of directories and files within the working directory, current dir \
                 is a working dir, i.e. `.` is the roor project"
                    .to_string(),
            ),
            parameters: json!({
                "type": "object",
                "properties": {
                    "directory_path": {
                        "type": "string",
                        "description": "The path of the directory where content to search is stored",
                    },
                },
                "required": ["directory_path"],
                "additionalProperties": false,
            })
            .as_object()
            .cloned(),
            strict: Some(true),
        }),
    }
});

pub static READ_REGION_CONTENT: Lazy<Tool> = Lazy::new(|| {
    Tool {
        r#type: "function".to_string(),
        function: Some(FunctionToCall {
            name: FunctionName::ReadRegionContent.to_string(),
            description: Some("Read the content of the particular region".to_string()),
            parameters: json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "The path of the file where content to search is stored",
                },
                "region": {
                    "type": "object",
                    "description": "The region in the file to read",
                    "properties": {
                        "a": {
                            "type": "integer",
                            "description": "The beginning point of the region to read, set -1 to read the file till the start",
                        },
                        "b": {
                            "type": "integer",
                            "description": "The ending point of the region to read, set -1 to read the file till the end",
                        },
                    },
                    "required": ["a", "b"],
                    "additionalProperties": false,
                },
            },
            "required": ["file_path", "region"],
            "additionalProperties": false,
        })
            .as_object()
            .cloned(),
            strict: Some(true),
        }),
    }
});
