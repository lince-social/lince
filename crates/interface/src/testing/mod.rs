//! Shared interface test protocol and its native Bevy backend.

pub use lince_host::testing::{
    Backend, Cancellation, Dispatch, Event, InterfaceSelection, Limits, Metrics, Outcome, Report,
    Route, Runner, Selector, Step,
};

#[cfg(feature = "native-presentation")]
pub mod ui;
