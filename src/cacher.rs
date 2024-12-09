use pyo3::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Error as SerdeError;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, Write};
use std::path::Path;

use types::{AssistantMessage, OpenAIMessage};

use crate::types;

#[derive(Serialize, Deserialize, Debug)]
pub struct Cacher {
    pub history_file: String,
    pub current_model_file: String,
    pub tokens_count_file: String,
}

#[derive(Debug, Deserialize)]
pub enum Message {
    OpenAI(OpenAIMessage),
    Assistant(AssistantMessage),
}

impl Cacher {
    pub fn new(module_name: &str, name: Option<&str>) -> Self {
        let cache_dir = Python::with_gil(|py| {
            let sublime = py
                .import(module_name)
                .expect("Failed to import the sublime module");

            sublime
                .call_method0("cache_path")
                .expect("Failed to call sublime.cache_path()")
                .extract::<String>()
                .expect("Failed to convert the Python result to a Rust String")
        });

        use std::path::{Path, PathBuf};

        let (_cache_dir, history_file, current_model_file, tokens_count_file) =
            if let Some(input) = name {
                if Path::new(input).is_absolute() {
                    let base_path = PathBuf::from(input);
                    (
                        base_path.to_string_lossy().into_owned(),
                        base_path
                            .join("chat_history.json")
                            .to_string_lossy()
                            .into_owned(),
                        base_path
                            .join("current_assistant.json")
                            .to_string_lossy()
                            .into_owned(),
                        base_path
                            .join("tokens_count.json")
                            .to_string_lossy()
                            .into_owned(),
                    )
                } else {
                    let name_prefix = format!("{}_", input);
                    (
                        cache_dir.clone(),
                        format!("{}/{}chat_history.json", cache_dir, name_prefix),
                        format!("{}/{}current_assistant.json", cache_dir, name_prefix),
                        format!("{}/{}tokens_count.json", cache_dir, name_prefix),
                    )
                }
            } else {
                (
                    cache_dir.clone(),
                    format!("{}/chat_history.json", cache_dir),
                    format!("{}/current_assistant.json", cache_dir),
                    format!("{}/tokens_count.json", cache_dir),
                )
            };

        Self {
            history_file,
            current_model_file,
            tokens_count_file,
        }
    }

    pub fn check_and_create(&self, path: &str) {
        if !Path::new(path).exists() {
            File::create(path).unwrap();
        }
    }

    pub fn read_entries(&self) -> Result<Vec<Message>, SerdeError> {
        self.check_and_create(&self.history_file);
        let file = File::open(&self.history_file).unwrap();
        let reader = std::io::BufReader::new(file);
        let mut entries = Vec::new();

        for line in reader.lines() {
            let line = line.unwrap();
            let obj: Message = serde_json::from_str(&line)?;
            entries.push(obj);
        }

        Ok(entries)
    }

    pub fn write_entry<T: Serialize>(&self, entry: &T) {
        let mut file = OpenOptions::new()
            .append(true)
            .create(true)
            .open(&self.history_file)
            .unwrap();

        let entry_json = serde_json::to_string(entry).unwrap();
        writeln!(file, "{}", entry_json).unwrap();
    }
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use std::fs::{self, File};
//     use std::io::Write;
//     use tempfile::TempDir;

// #[test]
// fn test_cacher_new_with_name() {
//     let name = "assistant_test";

//     // Use an absolute path with the given name
//     let cacher = Cacher::new(Some(name));

//     assert!(cacher.history_file.ends_with("chat_history.json"));
//     assert!(cacher
//         .current_model_file
//         .ends_with("current_assistant.json"));
//     assert!(cacher.tokens_count_file.ends_with("tokens_count.json"));
//     assert!(Path::new(&cacher.history_file).is_absolute());
// }

// #[test]
// fn test_cacher_new_without_name() {
//     let temp_dir = TempDir::new().unwrap();
//     let path = temp_dir.path().to_str().unwrap();

//     Python::with_gil(|py| {
//         let sublime_mock = py
//             .import("sublime_mock.py")
//             .expect("Expected 'sublime_cache_path' to exist!");

//         sublime_mock
//             .call_method1("set_cache_path", (path,))
//             .unwrap();
//     });

//     let cacher = Cacher::new(None);

//     assert!(cacher.history_file.ends_with("chat_history.json"));
//     assert!(cacher
//         .current_model_file
//         .ends_with("current_assistant.json"));
//     assert!(cacher.tokens_count_file.ends_with("tokens_count.json"));
// }

// #[test]
// fn test_cacher_check_and_create() {
//     let temp_dir = TempDir::new().unwrap();
//     let file_path = temp_dir.path().join("test_file.json");

//     let cacher = Cacher {
//         history_file: file_path.to_string_lossy().to_string(),
//         current_model_file: "".to_string(),
//         tokens_count_file: "".to_string(),
//     };

//     // Check and create the file
//     cacher.check_and_create(&cacher.history_file);

//     // Verify that the file was created
//     assert!(file_path.exists());
// }

// #[test]
// fn test_cacher_read_entries() {
//     let temp_dir = TempDir::new().unwrap();
//     let file_path = temp_dir.path().join("history.json");

//     // Populate file with JSON objects
//     let mut file = File::create(&file_path).unwrap();
//     writeln!(file, r#"{{"name": "Entry 1"}}"#).unwrap();
//     writeln!(file, r#"{{"name": "Entry 2"}}"#).unwrap();

//     let cacher = Cacher {
//         history_file: file_path.to_string_lossy().to_string(),
//         current_model_file: "".to_string(),
//         tokens_count_file: "".to_string(),
//     };

//     let entries = cacher.read_entries().unwrap();

//     assert_eq!(entries.len(), 2);
//     assert_eq!(entries[0]["name"], "Entry 1");
//     assert_eq!(entries[1]["name"], "Entry 2");
// }

// #[test]
// fn test_cacher_write_entry() {
//     let temp_dir = TempDir::new().unwrap();
//     let file_path = temp_dir.path().join("write_test.json");
//     let cacher = Cacher {
//         history_file: file_path.to_string_lossy().to_string(),
//         current_model_file: "".to_string(),
//         tokens_count_file: "".to_string(),
//     };

//     let entry = serde_json::json!({
//         "name": "New Entry",
//         "value": 42
//     });

//     // Write the entry
//     cacher.write_entry(&entry);

//     // Verify the file content
//     let content = fs::read_to_string(&file_path).unwrap();
//     assert!(content.contains(r#"{"name":"New Entry","value":42}"#));
// }

// #[test]
// fn test_combined_read_and_write_entries() {
//     let temp_dir = TempDir::new().unwrap();
//     let file_path = temp_dir.path().join("combined_test.json");
//     let cacher = Cacher {
//         history_file: file_path.to_string_lossy().to_string(),
//         current_model_file: "".to_string(),
//         tokens_count_file: "".to_string(),
//     };

//     let entry1 = serde_json::json!({
//         "name": "Entry 1"
//     });

//     let entry2 = serde_json::json!({
//         "name": "Entry 2"
//     });

//     // Write two entries
//     cacher.write_entry(&entry1);
//     cacher.write_entry(&entry2);

//     // Read the entries back
//     let entries = cacher.read_entries().unwrap();

//     assert_eq!(entries.len(), 2);
//     assert_eq!(entries[0]["name"], "Entry 1");
//     assert_eq!(entries[1]["name"], "Entry 2");
// }
// }
