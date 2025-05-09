use std::sync::Arc;

use once_cell::sync::Lazy;
use serde_json::json;
use strum_macros::{Display, EnumString};

use crate::openai_network_types::{FunctionToCall, Tool};

#[derive(EnumString, PartialEq, Display, Debug, Clone, Copy)]
#[strum(serialize_all = "snake_case")]
pub enum FunctionName {
    CreateFile,
    ApplyPatch,
    ReplaceTextForWholeFile,
    ReadRegionContent,
    GetWorkingDirectoryContent,
}

pub static FUNCTIONS: Lazy<Vec<Arc<Tool>>> = Lazy::new(|| {
    vec![
        // Arc::new((*CREATE_FILE).clone()),
        Arc::new((*REPLACE_TEXT_FOR_WHOLE_FILE).clone()),
        Arc::new((*APPLY_PATCH).clone()),
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

pub static APPLY_PATCH: Lazy<Tool> = Lazy::new(|| {
    Tool {
        r#type: "function".to_string(),
        function: Some(FunctionToCall {
            name: FunctionName::ApplyPatch.to_string(),
            description: Some(
                r#"Apply a patch to the given file.
                You must embed the file path in the patch itself:

                *** Begin Patch
                *** Update File: path/to/file.source
                - old line to replace
                + new replacement line
                *** End Patch

                Processing:
                1. Strip the Begin/End markers and extract the file path.
                2. Run simple find-and-replace for each `- old` → `+ new` pair.

                Returns `"Done!"` on success or an error message on failure.
                "#
                .to_string(),
            ),
            parameters: json!({
                "type": "object",
                "properties": {
                    "patch": {
                        "type": "string",
                        "description": "Your patch block including ***Begin/End and Update File header."
                    }
                },
                "required": ["patch"],
                "additionalProperties": false
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
            description: Some(r#"
            Read a region of a file by specifying start/end line numbers.
            Prefer reading large files in smaller chunks by narrowing the range.
            Only use a = -1 and b = -1 to fetch the entire file as a last resort."#.to_string()),
            parameters: json!({
                "type": "object",
                "properties": {
                    "file_path": {
                        "type": "string",
                        "description": "The path of the file to read",
                    },
                    "region": {
                        "type": "object",
                        "description": "Line range to read: specify `a` and `b` as start/end line indices, inclusive",
                        "properties": {
                            "a": {
                                "type": "integer",
                                "description": "Start line index (inclusive). Use -1 to start from the beginning of the file.",
                            },
                            "b": {
                                "type": "integer",
                                "description": "End line index (inclusive). Use -1 to read to the end of the file.",
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
