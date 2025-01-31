use std::fs::File;

use fern::Dispatch;

pub fn setup_logger(path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let log_file = File::create(path)?;

    Ok(Dispatch::new()
        .chain(log_file)
        .apply()?)
}
