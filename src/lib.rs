use pyo3::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Error as SerdeError;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, Write};
use std::path::Path;
pub mod types;
use types::{AssistantMessage, OpenAIMessage};

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
