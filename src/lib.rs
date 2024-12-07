// use pyo3::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Error as SerdeError;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, Write};
use std::path::Path;
pub mod types;

#[derive(Serialize, Deserialize, Debug)]
pub struct Cacher {
    pub history_file: String,
    pub current_model_file: String,
    pub tokens_count_file: String,
}

impl Cacher {
    pub fn new(name: Option<&str>) -> Self {
        let cache_dir = std::env::var("HOME").unwrap()
            + "/Library/Application Support/Sublime Text/Packages/OpenAI completion";
        fs::create_dir_all(&cache_dir).unwrap();

        let name_prefix = name.map_or(String::new(), |n| format!("{}_", n));
        let history_file = format!("{}/{}chat_history.json", cache_dir, name_prefix);
        let current_model_file = format!("{}/{}current_assistant.json", cache_dir, name_prefix);
        let tokens_count_file = format!("{}/{}tokens_count.json", cache_dir, name_prefix);

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

    pub fn read_entries(&self) -> Result<Vec<serde_json::Value>, SerdeError> {
        self.check_and_create(&self.history_file);
        let file = File::open(&self.history_file).unwrap();
        let reader = std::io::BufReader::new(file);
        let mut entries = Vec::new();

        for line in reader.lines() {
            let line = line.unwrap();
            let obj: serde_json::Value = serde_json::from_str(&line)?;
            entries.push(obj);
        }

        Ok(entries)
    }

    pub fn write_entry(&self, entry: &serde_json::Value) {
        let mut file = OpenOptions::new()
            .append(true)
            .create(true)
            .open(&self.history_file)
            .unwrap();
        writeln!(file, "{}", serde_json::to_string(entry).unwrap()).unwrap();
    }
}
