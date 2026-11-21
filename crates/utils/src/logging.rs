// non-lean-yet
// Scope: static QUIET, const LOG_INFO, enum LogEntry, fn set_quiet.
// BeautifulRust (baseline lines):
// - [review: §4 API] Check public bool parameters for a named mode or distinct operations. Sites
//   set_quiet:enabled (18).
// - [Clippy: clippy::allow_attributes] Use a reasoned expect when the diagnostic reliably fires.
//   Sites 12.
// - [Clippy: clippy::exhaustive_enums] Review whether public API evolution requires non_exhaustive.
//   Sites 13.
// - [Clippy: clippy::missing_errors_doc] State the observable failure cases for exported functions.
//   Sites 22.
// - [Clippy: clippy::print_stdout] Check CLI, Cargo, fixture, or library output contract before
//   changing output. Sites 41.
// - [rustc: missing_debug_implementations] type does not implement `std::fmt::Debug`; consider
//   adding `#[derive(Debug)]` or a manual implementation. Sites 13.
// - [rustc: missing_docs] Document the externally visible contracts where applicable. Sites 13, 14,
//   15, 18, 22, 39, 45.

use std::{
    io::{Error, ErrorKind},
    sync::atomic::{AtomicBool, Ordering},
};

use tracing_subscriber::{
    Layer, Registry, filter::LevelFilter, layer::SubscriberExt, util::SubscriberInitExt,
};

static QUIET: AtomicBool = AtomicBool::new(false);
const LOG_INFO: bool = false;

#[allow(
    dead_code,
    reason = "Retain the Info variant for optional callers even when it is unused"
)]
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
