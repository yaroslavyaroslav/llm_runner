use std::{
    fs::{File, OpenOptions},
    io::{self, BufRead, Write},
    path::Path,
};

use anyhow::Result;
use serde::{de::Error, Deserialize, Serialize};
use serde_json::Error as SerdeError;

use crate::sublime_python;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Cacher {
    pub current_model_file: String,
    pub history_file: String,
    pub tokens_count_file: String,
}

#[allow(unused)]
impl Cacher {
    pub fn new(name: Option<&str>) -> Self {
        let cache_dir =
            sublime_python::get_sublime_cache().unwrap_or("~/Library/Caches/Sublime Text/Cache".to_string());

        use std::path::{Path, PathBuf};

        let (history_file, current_model_file, tokens_count_file) = if let Some(input) = name {
            if Path::new(input).is_absolute() {
                let base_path = PathBuf::from(input);
                (
                    base_path
                        .join("_chat_history.json")
                        .to_string_lossy()
                        .into_owned(),
                    base_path
                        .join("_current_assistant.json")
                        .to_string_lossy()
                        .into_owned(),
                    base_path
                        .join("_tokens_count.json")
                        .to_string_lossy()
                        .into_owned(),
                )
            } else {
                let name_prefix = format!("{}_", input);
                (
                    format!(
                        "{}/{}chat_history.json",
                        cache_dir, name_prefix
                    ),
                    format!(
                        "{}/{}current_assistant.json",
                        cache_dir, name_prefix
                    ),
                    format!(
                        "{}/{}tokens_count.json",
                        cache_dir, name_prefix
                    ),
                )
            }
        } else {
            (
                format!("{}/chat_history.json", cache_dir),
                format!("{}/current_assistant.json", cache_dir),
                format!("{}/tokens_count.json", cache_dir),
            )
        };

        Self {
            current_model_file,
            history_file,
            tokens_count_file,
        }
    }

    fn create_file_if_not_exists(&self, path: &str) -> Result<()> {
        if !Path::new(path).exists() {
            File::create(path)?;
            println!("File created successfully.");
        }
        Ok(())
    }

    pub fn read_entries<T>(&self) -> Result<Vec<T>, SerdeError>
    where T: for<'de> Deserialize<'de> {
        self.create_file_if_not_exists(&self.history_file);

        let file = match File::open(&self.history_file) {
            Ok(file) => file,
            Err(_) => return Ok(Vec::new()),
        };

        let reader = std::io::BufReader::new(file);
        let mut entries = Vec::new();

        for (num, line) in reader.lines().enumerate() {
            let line = line.map_err(SerdeError::custom)?;
            match serde_json::from_str::<T>(&line) {
                Ok(obj) => entries.push(obj),
                Err(err) => {
                    eprintln!(
                        "Malformed line skipped: {} (Error: {})",
                        num, err
                    )
                }
            }
        }

        Ok(entries)
    }

    pub fn write_entry<T: Serialize>(&self, entry: &T) -> Result<()> {
        let entry_json = serde_json::to_string(entry).map_err(|e| {
            eprintln!("Error serializing entry: {}", e);
            io::Error::new(
                io::ErrorKind::Other,
                "Serialization error",
            )
        })?;

        let mut file = OpenOptions::new()
            .append(true)
            .create(true)
            .open(&self.history_file)?;

        writeln!(file, "{}", entry_json).map_err(|e| {
            eprintln!("Error writing to file: {}", e);
            io::Error::new(io::ErrorKind::Other, "Write error")
        })?;

        Ok(())
    }

    pub fn drop_first(&self, lines_num: usize) -> Result<()> {
        let file = File::open(&self.history_file)?;

        let reader = std::io::BufReader::new(file);
        let remaining_lines: Vec<_> = reader
            .lines()
            .skip(lines_num)
            .filter_map(Result::ok)
            .collect();

        let mut file = File::create(&self.history_file)?;

        for line in remaining_lines {
            writeln!(file, "{}", line).map_err(|e| {
                eprintln!("Error writing to file: {}", e);
                io::Error::new(io::ErrorKind::Other, "Write error")
            })?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs::File,
        io::{BufRead, BufReader, Write},
    };

    use serde::{Deserialize, Serialize};
    use tempfile::TempDir;

    use super::*;

    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct TestEntry {
        id: i32,
        name: String,
    }

    #[test]
    fn test_write_and_read_entries() {
        let temp_dir = TempDir::new().unwrap();
        let history_path = temp_dir
            .path()
            .join("test_history.json");
        let cacher = Cacher {
            history_file: history_path
                .to_string_lossy()
                .to_string(),
            current_model_file: "".to_string(),
            tokens_count_file: "".to_string(),
        };

        let entry1 = TestEntry {
            id: 1,
            name: "Alice".to_string(),
        };
        let entry2 = TestEntry {
            id: 2,
            name: "Bob".to_string(),
        };

        cacher
            .write_entry(&entry1)
            .ok();
        cacher
            .write_entry(&entry2)
            .ok();

        let file = File::open(&cacher.history_file).unwrap();
        let reader = BufReader::new(file);
        let lines: Vec<_> = reader.lines().collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(
            lines[0].as_ref().unwrap(),
            &serde_json::to_string(&entry1).unwrap()
        );
        assert_eq!(
            lines[1].as_ref().unwrap(),
            &serde_json::to_string(&entry2).unwrap()
        );

        let read_entries: Vec<TestEntry> = cacher.read_entries().unwrap();

        assert_eq!(read_entries.len(), 2);
        assert_eq!(read_entries[0], entry1);
        assert_eq!(read_entries[1], entry2);
    }

    #[test]
    fn test_read_empty_file() {
        let temp_dir = TempDir::new().unwrap();
        let history_path = temp_dir
            .path()
            .join("empty_history.json");
        let cacher = Cacher {
            history_file: history_path
                .to_string_lossy()
                .to_string(),
            current_model_file: "".to_string(),
            tokens_count_file: "".to_string(),
        };

        cacher
            .create_file_if_not_exists(&cacher.history_file)
            .ok();

        let read_entries: Vec<TestEntry> = cacher.read_entries().unwrap();

        assert!(read_entries.is_empty());
    }

    #[test]
    fn test_partial_or_corrupted_entries() {
        let temp_dir = TempDir::new().unwrap();
        let history_path = temp_dir
            .path()
            .join("corrupted_history.json");
        let cacher = Cacher {
            history_file: history_path
                .to_string_lossy()
                .to_string(),
            current_model_file: "".to_string(),
            tokens_count_file: "".to_string(),
        };

        let entry1 = TestEntry {
            id: 1,
            name: "Alice".to_string(),
        };

        let valid_line = serde_json::to_string(&entry1).unwrap();
        let corrupted_line = "{ id: 2, name: Bob }";

        let mut file = File::create(&cacher.history_file).unwrap();
        writeln!(file, "{}", valid_line).unwrap();
        writeln!(file, "{}", corrupted_line).unwrap();

        let read_entries: Vec<TestEntry> = cacher.read_entries().unwrap();

        assert_eq!(read_entries.len(), 1);
        assert_eq!(read_entries[0], entry1);
    }

    use crate::{
        openai_network_types::{Function, Roles, ToolCall},
        types::CacheEntry,
    };

    #[test]
    fn test_read_entries_with_mock_data() {
        let temp_dir = TempDir::new().unwrap();
        let history_path = temp_dir
            .path()
            .join("test_history.json");
        let cacher = Cacher {
            history_file: history_path
                .to_string_lossy()
                .into_owned(),
            current_model_file: "".to_string(),
            tokens_count_file: "".to_string(),
        };

        // Mock JSON entries to write to the file
        let mock_entries = vec![
            r#"{"content":"Test request acknowledged.","role":"assistant"}"#,
            r#"{"content":"This is the test request, provide me 3 words response","role":"user"}"#,
            r#"{"role":"assistant","tool_call":{"id":"call_f4Ixx2ruFvbbqifrMKZ8Cxju","type":"function","function":{"name":"create_file","arguments":"{\"file_path\":\"test_response.txt\"}"}}}"#,
            r#"{"role":"tool","tool_call_id":"call_f4Ixx2ruFvbbqifrMKZ8Cxju", "content": "created"}"#,
        ];

        // Write mock entries to the cache file
        {
            let mut file = File::create(&cacher.history_file).unwrap();
            for entry in mock_entries {
                writeln!(file, "{}", entry).unwrap();
            }
        }

        // Read entries from the cache
        let read_entries: Vec<CacheEntry> = cacher.read_entries().unwrap();

        // Check the contents of the read entries
        assert_eq!(read_entries.len(), 4);

        // Checking the first entry
        assert_eq!(
            read_entries[0],
            CacheEntry {
                content: Some("Test request acknowledged.".to_string()),
                role: Roles::Assistant,
                tool_call: None,
                path: None,
                scope: None,
                tool_call_id: None
            }
        );

        // Checking the second entry
        assert_eq!(
            read_entries[1],
            CacheEntry {
                content: Some("This is the test request, provide me 3 words response".to_string()),
                role: Roles::User,
                tool_call: None,
                path: None,
                scope: None,
                tool_call_id: None
            }
        );

        // Checking the third entry
        assert_eq!(
            read_entries[2],
            CacheEntry {
                content: None,
                role: Roles::Assistant,
                tool_call: Some(ToolCall {
                    id: "call_f4Ixx2ruFvbbqifrMKZ8Cxju".to_string(),
                    r#type: "function".to_string(),
                    function: Function {
                        name: "create_file".to_string(),
                        arguments: "{\"file_path\":\"test_response.txt\"}".to_string(),
                    }
                }),
                path: None,
                scope: None,
                tool_call_id: None
            }
        );

        assert_eq!(
            read_entries[3],
            CacheEntry {
                content: Some("created".to_string()),
                role: Roles::Tool,
                tool_call: None,
                path: None,
                scope: None,
                tool_call_id: Some("call_f4Ixx2ruFvbbqifrMKZ8Cxju".to_string())
            }
        );
    }
}
