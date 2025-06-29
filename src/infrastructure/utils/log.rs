use chrono::{DateTime, Utc};
use std::{
    fs::{OpenOptions, create_dir_all},
    io::{ErrorKind, Write},
    path::PathBuf,
};

pub enum LogEntry {
    Error(ErrorKind, String),
    Message(String),
}

pub fn log(entry: LogEntry) {
    let timestamp: DateTime<Utc> = Utc::now();
    let date = timestamp.format("%Y-%m-%d").to_string();
    let time = timestamp.format("%Y-%m-%d %H:%M:%S").to_string();

    let log_dir: PathBuf = [
        "logs",
        &timestamp.format("%Y").to_string(),
        &timestamp.format("%m").to_string(),
    ]
    .iter()
    .collect();
    let log_file = log_dir.join(format!("{}.log", date));

    if let Err(e) = create_dir_all(&log_dir) {
        eprintln!(
            "{} | [LOG ERROR]: Failed to create log directory: {}",
            time, e
        );
        return;
    }

    let formatted = match entry {
        LogEntry::Error(kind, message) => format!(
            "{} | [ERROR]: Kind: {:?} | Message: {}",
            time, kind, message
        ),
        LogEntry::Message(message) => format!("{} | [MESSAGE]: {:?}", time, message),
    };

    match OpenOptions::new().create(true).append(true).open(&log_file) {
        Ok(mut file) => {
            if let Err(e) = writeln!(file, "{}", formatted) {
                eprintln!("{} | [LOG ERROR]: Failed to write to log file: {}", time, e);
            }
        }
        Err(e) => eprintln!("{} | [LOG ERROR]: Failed to open log file: {}", time, e),
    }
}
