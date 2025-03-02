use std::{
    fs::{File, OpenOptions},
    io::{BufRead, Write},
    path::Path,
};

use anyhow::Result;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Cacher {
    pub current_model_file: String,
    pub history_file: String,
    pub tokens_count_file: String,
}

#[allow(unused)]
impl Cacher {
    pub fn new(name: &str) -> Self {
        let cache_dir = Cacher::sublime_cache();

        use std::path::{Path, PathBuf};

        // TODO: Seems that this conditioning is useless and should be removed by expecting the absolute path only.
        let (history_file, current_model_file, tokens_count_file) = if Path::new(name).is_absolute() {
            let base_path = PathBuf::from(name);
            (
                base_path
                    .join("chat_history.jl")
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
            let name_prefix = format!("{}_", name);
            (
                format!(
                    "{}/{}chat_history.jl",
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
        };

        Self {
            current_model_file,
            history_file,
            tokens_count_file,
        }
    }

    fn create_file_if_not_exists(path: &str) -> Result<()> {
        if !Path::new(path).exists() {
            File::create(path)?;
            println!("File created successfully.");
        }
        Ok(())
    }

    pub fn read_entries<T>(&self) -> Result<Vec<T>>
    where T: for<'de> Deserialize<'de> {
        Self::create_file_if_not_exists(&self.history_file);

        let file = match File::open(&self.history_file) {
            Ok(file) => file,
            Err(_) => return Ok(Vec::new()),
        };

        let reader = std::io::BufReader::new(file);
        let mut entries = Vec::new();

        reader
            .lines()
            .enumerate()
            .for_each(|(num, line)| {
                serde_json::from_str::<T>(&line.unwrap_or_default())
                    .map(|obj| entries.push(obj))
                    .unwrap_or_else(|err| {
                        eprintln!(
                            "Malformed line skipped: {} (Error: {})",
                            num, err
                        )
                    });
            });

        Ok(entries)
    }

    pub fn write_entry<T: Serialize>(&self, entry: &T) -> Result<()> {
        let entry_json = serde_json::to_string(entry)?;

        let mut file = OpenOptions::new()
            .append(true)
            .create(true)
            .open(&self.history_file)?;

        writeln!(file, "{}", entry_json)?;

        Ok(())
    }

    pub fn write_model<T: Serialize>(&self, model: &T) -> Result<()> {
        let model_json = serde_json::to_string(model)?;

        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&self.current_model_file)?;

        writeln!(file, "{}", model_json)?;

        Ok(())
    }

    pub fn read_model<T: DeserializeOwned>(&self) -> Result<T> {
        Self::create_file_if_not_exists(&self.current_model_file);

        let file = File::open(&self.current_model_file)?;
        let reader = std::io::BufReader::new(file);

        // Read the file and deserialize it into the desired type `T`
        let model: T = serde_json::from_reader(reader)?;

        Ok(model)
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
            writeln!(file, "{}", line)?;
        }

        Ok(())
    }

    pub fn drop_all(&self) -> Result<()> {
        let mut file = File::create(&self.history_file)?;
        Ok(())
    }

    #[cfg(test)]
    fn sublime_cache() -> String { "~/Library/Caches/Sublime Text/Cache".to_string() }

    #[cfg(not(test))]
    fn sublime_cache() -> String {
        "~/Library/Caches/Sublime Text/Cache".to_string()
        // crate::sublime_python::get_sublime_cache()
        //     .unwrap_or("~/Library/Caches/Sublime Text/Cache".to_string())
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
    fn test_is_sync_and_send() {
        fn is_sync<T: Sync>() {}
        fn is_send<T: Send>() {}

        is_sync::<Cacher>();
        is_send::<Cacher>();
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

        Cacher::create_file_if_not_exists(&cacher.history_file).ok();

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
        types::{ApiType, AssistantSettings, CacheEntry, PromptMode, ReasonEffort},
    };

    #[test]
    fn test_assistant_settings() {
        let mut settings = AssistantSettings::default();

        settings.api_type = ApiType::OpenAi;
        settings.reasoning_effort = Some(ReasonEffort::High);
        settings.name = "Example".to_string();
        settings.chat_model = "gpt-4o-mini".to_string();
        settings.assistant_role = Some("Some Role".to_string());
        settings.url = "https://models.inference.ai.azure.com/path/to".to_string();
        settings.token = Some("some_token".to_string());
        settings.temperature = Some(0.7);
        settings.max_tokens = None;
        settings.max_completion_tokens = Some(2048);
        settings.top_p = Some(1.0);
        settings.frequency_penalty = Some(2.0);
        settings.presence_penalty = Some(3.0);
        settings.tools = Some(true);
        settings.parallel_tool_calls = Some(false);
        settings.stream = false;
        settings.advertisement = false;
        settings.output_mode = PromptMode::View;
        settings.api_type = ApiType::OpenAi;

        let encoded = serde_json::to_string(&settings).unwrap();
        let decoded = serde_json::from_str::<AssistantSettings>(&encoded).unwrap();

        assert_eq!(decoded.api_type, ApiType::OpenAi);
        assert_eq!(
            decoded.reasoning_effort,
            Some(ReasonEffort::High)
        );
        assert_eq!(decoded.name, "Example".to_string());
        assert_eq!(
            decoded.chat_model,
            "gpt-4o-mini".to_string()
        );
        assert_eq!(
            decoded.assistant_role,
            Some("Some Role".to_string())
        );
        assert_eq!(
            decoded.url,
            "https://models.inference.ai.azure.com/path/to".to_string()
        );
        assert_eq!(
            decoded.token,
            Some("some_token".to_string())
        );
        assert_eq!(decoded.temperature, Some(0.7));
        assert_eq!(decoded.max_tokens, None);
        assert_eq!(
            decoded.max_completion_tokens,
            Some(2048)
        );
        assert_eq!(decoded.top_p, Some(1.0));
        assert_eq!(decoded.frequency_penalty, Some(2.0));
        assert_eq!(decoded.presence_penalty, Some(3.0));
        assert_eq!(decoded.tools, Some(true));
        assert_eq!(decoded.parallel_tool_calls, Some(false));
        assert!(!decoded.stream);
        assert!(!decoded.advertisement);
        assert_eq!(decoded.output_mode, PromptMode::View);
    }

    #[test]
    fn test_assistant_settings_write_read() {
        use tempfile::TempDir;
        // Create a temporary directory and file.
        let temp_dir = TempDir::new().unwrap();
        let model_path = temp_dir
            .path()
            .join("current_assistant.json");
        let cacher = Cacher {
            history_file: "".to_string(),
            current_model_file: model_path
                .to_string_lossy()
                .into_owned(),
            tokens_count_file: "".to_string(),
        };

        let mut settings = AssistantSettings::default();

        // Set fields
        settings.api_type = ApiType::OpenAi;
        settings.reasoning_effort = Some(ReasonEffort::High);
        settings.name = "Example".to_string();
        settings.chat_model = "gpt-4o-mini".to_string();
        settings.assistant_role = Some("Some Role".to_string());
        settings.url = "https://models.inference.ai.azure.com/path/to".to_string();
        settings.token = Some("some_token".to_string());
        settings.temperature = Some(0.7);
        settings.max_tokens = None;
        settings.max_completion_tokens = Some(2048);
        settings.top_p = Some(1.0);
        settings.frequency_penalty = Some(2.0);
        settings.presence_penalty = Some(3.0);
        // Assuming tools here as Option<bool>
        settings.tools = Some(true);
        settings.parallel_tool_calls = Some(false);
        settings.stream = false;
        settings.advertisement = false;
        settings.output_mode = PromptMode::View;
        settings.api_type = ApiType::OpenAi;

        // Write the settings model to file.
        let _ = cacher.write_model(&settings);

        // Read the settings model back from file.
        let settings = cacher
            .read_model::<AssistantSettings>()
            .unwrap();

        // Assert that all fields match the original settings.
        assert_eq!(settings.api_type, ApiType::OpenAi);
        assert_eq!(
            settings.reasoning_effort,
            Some(ReasonEffort::High)
        );
        assert_eq!(settings.name, "Example".to_string());
        assert_eq!(
            settings.chat_model,
            "gpt-4o-mini".to_string()
        );
        assert_eq!(
            settings.assistant_role,
            Some("Some Role".to_string())
        );
        assert_eq!(
            settings.url,
            "https://models.inference.ai.azure.com/path/to".to_string()
        );
        assert_eq!(
            settings.token,
            Some("some_token".to_string())
        );
        assert_eq!(settings.temperature, Some(0.7));
        assert_eq!(settings.max_tokens, None);
        assert_eq!(
            settings.max_completion_tokens,
            Some(2048)
        );
        assert_eq!(settings.top_p, Some(1.0));
        assert_eq!(settings.frequency_penalty, Some(2.0));
        assert_eq!(settings.presence_penalty, Some(3.0));
        assert_eq!(settings.tools, Some(true));
        assert_eq!(
            settings.parallel_tool_calls,
            Some(false)
        );
        assert!(!settings.stream);
        assert!(!settings.advertisement);
        assert_eq!(settings.output_mode, PromptMode::View);
    }

    #[test]
    fn test_read_entries_with_mock_data() {
        let temp_dir = TempDir::new().unwrap();
        let history_path = temp_dir
            .path()
            .join("test_history.jl");
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
            r#"{"role":"assistant","tool_calls":[{"id":"call_f4Ixx2ruFvbbqifrMKZ8Cxju","type":"function","function":{"name":"create_file","arguments":"{\"file_path\":\"test_response.txt\"}"}}]}"#,
            r#"{"role":"tool","tool_call_id":"call_f4Ixx2ruFvbbqifrMKZ8Cxju", "content": "created"}"#,
        ];

        // Write mock entries to the cache file
        {
            let mut file = File::create(&cacher.history_file).unwrap();
            for entry in mock_entries {
                writeln!(file, "{}", entry).unwrap();
            }
        }

        let file = File::open(&cacher.history_file).unwrap();
        let reader = BufReader::new(file);
        let lines: Vec<String> = reader
            .lines()
            .filter_map(Result::ok)
            .collect();

        // Assert that there are exactly 4 lines.
        assert_eq!(lines.len(), 4);

        // Read entries from the cache
        let read_entries: Vec<CacheEntry> = cacher.read_entries().unwrap();

        // Check the contents of the read entries
        assert_eq!(read_entries.len(), 4);

        // Checking the first entry
        assert_eq!(
            read_entries[0],
            CacheEntry {
                content: Some("Test request acknowledged.".to_string()),
                thinking: None,
                role: Roles::Assistant,
                tool_calls: None,
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
                thinking: None,
                role: Roles::User,
                tool_calls: None,
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
                thinking: None,
                role: Roles::Assistant,
                tool_calls: Some(vec![ToolCall {
                    id: "call_f4Ixx2ruFvbbqifrMKZ8Cxju".to_string(),
                    r#type: "function".to_string(),
                    function: Function {
                        name: "create_file".to_string(),
                        arguments: "{\"file_path\":\"test_response.txt\"}".to_string(),
                    }
                }]),
                path: None,
                scope: None,
                tool_call_id: None
            }
        );

        assert_eq!(
            read_entries[3],
            CacheEntry {
                content: Some("created".to_string()),
                thinking: None,
                role: Roles::Tool,
                tool_calls: None,
                path: None,
                scope: None,
                tool_call_id: Some("call_f4Ixx2ruFvbbqifrMKZ8Cxju".to_string())
            }
        );
    }
}
