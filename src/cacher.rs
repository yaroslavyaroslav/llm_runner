use serde::de::Error;
use serde::{Deserialize, Serialize};
use serde_json::Error as SerdeError;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, Write};
use std::path::Path;

#[derive(Debug, Clone)]
pub(crate) struct Cacher {
    pub current_model_file: String,
    pub history_file: String,
    pub tokens_count_file: String,
}

impl Cacher {
    pub fn new(subl_cach_path: &str, name: Option<&str>) -> Self {
        let cache_dir = subl_cach_path.to_string();

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
                    // cache_dir.clone(),
                    format!("{}/{}chat_history.json", cache_dir, name_prefix),
                    format!("{}/{}current_assistant.json", cache_dir, name_prefix),
                    format!("{}/{}tokens_count.json", cache_dir, name_prefix),
                )
            }
        } else {
            (
                // cache_dir.clone(),
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

    fn check_and_create(&self, path: &str) {
        if !Path::new(path).exists() {
            File::create(path).unwrap();
        }
    }

    pub(crate) fn read_entries<T>(&self) -> Result<Vec<T>, SerdeError>
    where
        T: for<'de> Deserialize<'de>,
    {
        self.check_and_create(&self.history_file);
        let file = File::open(&self.history_file).unwrap();
        let reader = std::io::BufReader::new(file);
        let mut entries = Vec::new();

        for line in reader.lines() {
            let line = line.map_err(SerdeError::custom)?;
            match serde_json::from_str::<T>(&line) {
                Ok(obj) => entries.push(obj),
                Err(err) => eprintln!("Malformed line skipped: {} (Error: {})", line, err),
            }
        }
        Ok(entries)
    }

    pub(crate) fn write_entry<T: Serialize>(&self, entry: &T) {
        let mut file = OpenOptions::new()
            .append(true)
            .create(true)
            .open(&self.history_file)
            .unwrap();

        let entry_json = serde_json::to_string(entry).unwrap();
        writeln!(file, "{}", entry_json).unwrap();
    }

    pub(crate) fn drop_first(&self, lines_num: usize) {
        if let Ok(file) = File::open(&self.history_file) {
            let reader = std::io::BufReader::new(file);
            let remaining_lines: Vec<_> = reader
                .lines()
                .skip(lines_num)
                .filter_map(Result::ok)
                .collect();

            let mut file = File::create(&self.history_file).unwrap();
            for line in remaining_lines {
                writeln!(file, "{}", line).unwrap();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    use std::fs::File;
    use std::io::{BufRead, BufReader, Write};
    use tempfile::TempDir;

    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct TestEntry {
        id: i32,
        name: String,
    }

    #[test]
    fn test_write_and_read_entries() {
        let temp_dir = TempDir::new().unwrap();
        let history_path = temp_dir.path().join("test_history.json");
        let cacher = Cacher {
            history_file: history_path.to_string_lossy().to_string(),
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

        cacher.write_entry(&entry1);
        cacher.write_entry(&entry2);

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
        let history_path = temp_dir.path().join("empty_history.json");
        let cacher = Cacher {
            history_file: history_path.to_string_lossy().to_string(),
            current_model_file: "".to_string(),
            tokens_count_file: "".to_string(),
        };

        cacher.check_and_create(&cacher.history_file);

        let read_entries: Vec<TestEntry> = cacher.read_entries().unwrap();

        assert!(read_entries.is_empty());
    }

    #[test]
    fn test_partial_or_corrupted_entries() {
        let temp_dir = TempDir::new().unwrap();
        let history_path = temp_dir.path().join("corrupted_history.json");
        let cacher = Cacher {
            history_file: history_path.to_string_lossy().to_string(),
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
}
