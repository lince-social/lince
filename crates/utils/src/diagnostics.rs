mod journal;

pub use journal::Journal;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex, OnceLock};
use tracing::{
    Event, Subscriber,
    field::{Field, Visit},
};
use tracing_subscriber::{Layer, layer::Context};

const CAPACITY: usize = 64;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Notice {
    pub id: u64,
    pub source: String,
    pub message: String,
    pub occurrences: u64,
    pub last_seen: String,
}

#[derive(Default)]
struct State {
    notices: Vec<Notice>,
    revision: u64,
    next_id: u64,
    listeners: Vec<(u64, Arc<dyn Fn() + Send + Sync>)>,
    next_listener: u64,
}

#[derive(Clone, Default)]
pub struct Diagnostics(Arc<Mutex<State>>);

impl Diagnostics {
    pub fn global() -> Self {
        static LOG: OnceLock<Diagnostics> = OnceLock::new();
        LOG.get_or_init(Self::default).clone()
    }

    pub fn snapshot(&self) -> (u64, Vec<Notice>) {
        let state = self.0.lock().unwrap_or_else(|error| error.into_inner());
        (state.revision, state.notices.clone())
    }

    pub fn revision(&self) -> u64 {
        self.0
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .revision
    }

    pub fn report(&self, source: &str, message: &str) {
        let source = truncate(source, 128);
        let message = truncate(message, 2048);
        self.change(|state| {
            let previous = state
                .notices
                .iter()
                .position(|notice| notice.source == source && notice.message == message);
            let mut notice = if let Some(index) = previous {
                state.notices.remove(index)
            } else {
                state.next_id = state.next_id.saturating_add(1);
                Notice {
                    id: state.next_id,
                    source,
                    message,
                    occurrences: 0,
                    last_seen: String::new(),
                }
            };
            notice.occurrences = notice.occurrences.saturating_add(1);
            notice.last_seen = chrono::Utc::now().to_rfc3339();
            state.notices.push(notice);
            if state.notices.len() > CAPACITY {
                state.notices.remove(0);
            }
            true
        });
    }

    pub fn dismiss(&self, id: u64) {
        self.change(|state| {
            let length = state.notices.len();
            state.notices.retain(|notice| notice.id != id);
            length != state.notices.len()
        });
    }

    pub fn subscribe(&self, listener: impl Fn() + Send + Sync + 'static) -> Subscription {
        let mut state = self.0.lock().unwrap_or_else(|error| error.into_inner());
        state.next_listener += 1;
        let id = state.next_listener;
        state.listeners.push((id, Arc::new(listener)));
        Subscription {
            log: self.clone(),
            id,
        }
    }

    fn change(&self, change: impl FnOnce(&mut State) -> bool) {
        let listeners = {
            let mut state = self.0.lock().unwrap_or_else(|error| error.into_inner());
            if !change(&mut state) {
                return;
            }
            state.revision = state.revision.wrapping_add(1);
            state
                .listeners
                .iter()
                .map(|(_, listener)| listener.clone())
                .collect::<Vec<_>>()
        };
        for listener in listeners {
            listener();
        }
    }
}

pub struct Subscription {
    log: Diagnostics,
    id: u64,
}

impl Drop for Subscription {
    fn drop(&mut self) {
        self.log
            .0
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .listeners
            .retain(|(id, _)| *id != self.id);
    }
}

fn truncate(value: &str, limit: usize) -> String {
    let mut end = value.len().min(limit);
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].into()
}

pub(crate) struct DiagnosticLayer;

fn application_target(target: &str) -> bool {
    matches!(
        target.split("::").next(),
        Some(
            "lince"
                | "lince_interface"
                | "nucleus"
                | "anicca"
                | "cell"
                | "engine"
                | "store"
                | "protein"
                | "transport"
                | "utils"
        )
    )
}

impl<S: Subscriber> Layer<S> for DiagnosticLayer {
    fn on_event(&self, event: &Event<'_>, _: Context<'_, S>) {
        if *event.metadata().level() > tracing::Level::WARN
            || !application_target(event.metadata().target())
        {
            return;
        }
        let mut fields = Fields::default();
        event.record(&mut fields);
        let message = match (fields.message.is_empty(), fields.error.is_empty()) {
            (false, false) => format!("{}: {}", fields.message, fields.error),
            (false, true) => fields.message,
            (true, false) => fields.error,
            (true, true) => return,
        };
        Diagnostics::global().report(event.metadata().target(), &message);
    }
}

#[derive(Default)]
struct Fields {
    message: String,
    error: String,
}

impl Visit for Fields {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        match field.name() {
            "message" => self.message = format!("{value:?}"),
            "error" => self.error = format!("{value:?}"),
            _ => {}
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        match field.name() {
            "message" => self.message = value.into(),
            "error" => self.error = value.into(),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn bounded_deduplicated_notices_wake_listeners_and_dismiss() {
        let log = Diagnostics::default();
        let wakes = Arc::new(AtomicUsize::new(0));
        let counter = wakes.clone();
        let subscription = log.subscribe(move || {
            counter.fetch_add(1, Ordering::Relaxed);
        });
        log.report("cell", "Cannot open the key");
        log.report("cell", "Cannot open the key");
        let (_, notices) = log.snapshot();
        assert_eq!(notices.len(), 1);
        assert_eq!(notices[0].occurrences, 2);
        log.dismiss(notices[0].id);
        assert!(log.snapshot().1.is_empty());
        assert_eq!(wakes.load(Ordering::Relaxed), 3);
        drop(subscription);
        for index in 0..100 {
            log.report("cell", &index.to_string());
        }
        assert_eq!(log.snapshot().1.len(), CAPACITY);
        assert_eq!(wakes.load(Ordering::Relaxed), 3);
        log.report("cell", &"é".repeat(4000));
        assert!(log.snapshot().1.last().unwrap().message.len() <= 2048);
    }

    #[test]
    fn captures_application_errors_without_unrelated_fields_or_environment_warnings() {
        use tracing_subscriber::layer::SubscriberExt;
        let subscriber = tracing_subscriber::registry().with(DiagnosticLayer);
        tracing::subscriber::with_default(subscriber, || {
            tracing::warn!(target: "cell::test", error = "disk denied", password = "do not retain", "Cannot save");
            tracing::warn!(target: "wgpu", "test graphics warning");
            tracing::info!(target: "cell::test", "test normal startup");
        });
        let notices = Diagnostics::global().snapshot().1;
        assert!(
            notices
                .iter()
                .any(|notice| notice.message == "Cannot save: disk denied")
        );
        assert!(
            !notices
                .iter()
                .any(|notice| notice.message.contains("do not retain")
                    || notice.message.contains("test graphics warning")
                    || notice.message.contains("test normal startup"))
        );
    }
}
