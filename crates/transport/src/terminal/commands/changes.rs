//! Coalesced notifications for command history and run output.

use std::{
    collections::HashMap,
    fmt,
    ptr::eq,
    sync::{Arc, Mutex, Weak},
};

use tokio::sync::watch;

use super::CommandHost;

#[derive(Default)]
struct Topics {
    history: Option<watch::Sender<()>>,
    runs: HashMap<String, watch::Sender<()>>,
}

#[derive(Default)]
pub(super) struct Registry {
    topics: Mutex<HashMap<String, Topics>>,
}

/// Identity of a command notification registry without keeping the host alive.
#[derive(Clone, Debug)]
pub struct Source(Weak<Registry>);

impl Source {
    /// Checks whether both sources identify the same registry.
    pub fn same(&self, other: &Self) -> bool {
        self.0.ptr_eq(&other.0)
    }

    /// Checks whether this is the host's notification source.
    pub fn matches(&self, host: &CommandHost) -> bool {
        eq(self.0.as_ptr(), Arc::as_ptr(&host.0.changes))
    }
}

/// A subscription to a command history or a particular run's output.
pub struct Changes {
    // Field order closes the receiver before registration cleanup counts subscribers.
    receiver: watch::Receiver<()>,
    registration: Registration,
}

struct Registration {
    source: Source,
    command: String,
    run: Option<String>,
}

impl fmt::Debug for Changes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Changes")
            .field("observes_run", &self.registration.run.is_some())
            .finish_non_exhaustive()
    }
}

impl Changes {
    /// Returns the source identity for comparing subscriptions.
    pub fn source(&self) -> Source {
        self.registration.source.clone()
    }

    /// Waits for a change and returns false after the notification source closes.
    pub async fn changed(&mut self) -> bool {
        self.receiver.changed().await.is_ok()
    }
}

impl Registry {
    pub(super) fn source(self: &Arc<Self>) -> Source {
        Source(Arc::downgrade(self))
    }

    pub(super) fn observe(
        self: &Arc<Self>,
        command: &str,
        run: Option<&str>,
    ) -> Result<Changes, String> {
        let mut topics = self.topics.lock().map_err(|error| error.to_string())?;
        let topic = topics.entry(command.to_owned()).or_default();
        let sender = if let Some(run) = run {
            topic
                .runs
                .entry(run.to_owned())
                .or_insert_with(|| watch::channel(()).0)
        } else {
            topic.history.get_or_insert_with(|| watch::channel(()).0)
        };
        Ok(Changes {
            receiver: sender.subscribe(),
            registration: Registration {
                source: self.source(),
                command: command.into(),
                run: run.map(str::to_owned),
            },
        })
    }

    pub(super) fn notify(&self, command: &str, run: &str, history: bool) {
        if let Ok(topics) = self.topics.lock()
            && let Some(topic) = topics.get(command)
        {
            if history && let Some(sender) = &topic.history {
                sender.send_replace(());
            }
            if let Some(sender) = topic.runs.get(run) {
                sender.send_replace(());
            }
        }
    }
}

#[cfg(test)]
mod tests;

impl Drop for Registration {
    fn drop(&mut self) {
        let Some(registry) = self.source.0.upgrade() else {
            return;
        };
        let Ok(mut topics) = registry.topics.lock() else {
            return;
        };
        let Some(topic) = topics.get_mut(&self.command) else {
            return;
        };
        if let Some(run) = &self.run {
            if topic
                .runs
                .get(run)
                .is_some_and(|sender| sender.receiver_count() == 0)
            {
                topic.runs.remove(run);
            }
        } else if topic
            .history
            .as_ref()
            .is_some_and(|sender| sender.receiver_count() == 0)
        {
            topic.history = None;
        }
        if topic.history.is_none() && topic.runs.is_empty() {
            topics.remove(&self.command);
        }
    }
}
