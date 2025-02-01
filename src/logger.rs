use std::fs::File;

use fern::Dispatch;

#[allow(dead_code)]
pub fn setup_logger(path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let log_file = File::create(path)?;

    Ok(Dispatch::new()
        .chain(log_file)
        .apply()?)
}
