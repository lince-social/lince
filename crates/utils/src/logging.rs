use chrono::{DateTime, Utc};
use std::{
    fs::{OpenOptions, create_dir_all},
    io::{ErrorKind, Write},
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
};

static QUIET: AtomicBool = AtomicBool::new(false);

#[allow(dead_code)]
pub enum LogEntry {
    Error(ErrorKind, String),
    Info(String),
}

pub fn set_quiet(enabled: bool) {
    QUIET.store(enabled, Ordering::Relaxed);
}

pub fn status(message: impl AsRef<str>) {
    if !QUIET.load(Ordering::Relaxed) {
        println!("{}", message.as_ref());
    }
}

pub fn error(message: impl AsRef<str>) {
    eprintln!("{}", message.as_ref());
}

pub fn log(entry: LogEntry) {
    let timestamp: DateTime<Utc> = Utc::now();
    let date = timestamp.format("%Y-%m-%d").to_string();
    let time = timestamp.format("%Y-%m-%d %H:%M:%S").to_string();
    let Some(config_dir) = dirs::config_dir() else {
        error(format!(
            "{} | [LOG ERROR]: Unable to resolve user config directory",
            time
        ));
        return;
    };
    let log_dir: PathBuf = config_dir
        .join("lince")
        .join("log")
        .join(timestamp.format("%Y").to_string())
        .join(timestamp.format("%m").to_string());
    let log_file = log_dir.join(format!("{}.log", date));

    if let Err(e) = create_dir_all(&log_dir) {
        error(format!(
            "{} | [LOG ERROR]: Failed to create log directory: {}",
            time, e
        ));
        return;
    }

    let formatted = match entry {
        LogEntry::Error(kind, message) => format!(
            "{} | [ERROR]: Kind: {:?} | Message: {}",
            time, kind, message
        ),
        LogEntry::Info(info) => format!("{} | [INFO]: {:#?}", time, info),
    };

    match OpenOptions::new().create(true).append(true).open(&log_file) {
        Ok(mut file) => {
            if let Err(e) = writeln!(file, "{}", formatted) {
                error(format!(
                    "{} | [LOG ERROR]: Failed to write to log file: {}",
                    time, e
                ));
            }
        }
        Err(e) => error(format!(
            "{} | [LOG ERROR]: Failed to open log file: {}",
            time, e
        )),
    }
}
