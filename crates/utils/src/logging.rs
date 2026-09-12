use std::{
    io::{Error, ErrorKind},
    sync::atomic::{AtomicBool, Ordering},
};
use tracing_subscriber::{
    Layer, Registry, filter::LevelFilter, layer::SubscriberExt, util::SubscriberInitExt,
};

static QUIET: AtomicBool = AtomicBool::new(false);
const LOG_INFO: bool = false;

#[allow(dead_code)]
pub enum LogEntry {
    Error(ErrorKind, String),
    Info(String),
}

pub fn set_quiet(enabled: bool) {
    QUIET.store(enabled, Ordering::Relaxed);
}

pub fn init() -> Result<(), Error> {
    let level = if LOG_INFO {
        LevelFilter::INFO
    } else {
        LevelFilter::WARN
    };
    Registry::default()
        .with(crate::diagnostics::DiagnosticLayer.with_filter(LevelFilter::WARN))
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(std::io::stderr)
                .with_filter(level),
        )
        .try_init()
        .map_err(Error::other)
}

pub fn status(message: impl AsRef<str>) {
    if !QUIET.load(Ordering::Relaxed) {
        println!("{}", message.as_ref());
    }
}

pub fn log(entry: LogEntry) {
    match entry {
        LogEntry::Error(kind, message) => tracing::error!(?kind, %message),
        LogEntry::Info(message) if LOG_INFO => tracing::info!(%message),
        LogEntry::Info(_) => {}
    }
}
