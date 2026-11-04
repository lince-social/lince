use super::*;
use std::sync::Weak;
use tokio::sync::watch;

#[derive(Default)]
struct Topics {
    history: Option<watch::Sender<()>>,
    runs: HashMap<String, watch::Sender<()>>,
}

#[derive(Default)]
pub(super) struct Registry {
    topics: Mutex<HashMap<String, Topics>>,
}

#[derive(Clone)]
pub struct Source(Weak<Registry>);

impl Source {
    pub fn same(&self, other: &Self) -> bool {
        self.0.ptr_eq(&other.0)
    }

    pub fn matches(&self, host: &CommandHost) -> bool {
        self.0.ptr_eq(&Arc::downgrade(&host.0.changes))
    }
}

pub struct Changes {
    source: Source,
    command: String,
    run: Option<String>,
    receiver: Option<watch::Receiver<()>>,
}

impl Changes {
    pub fn source(&self) -> Source {
        self.source.clone()
    }

    pub async fn changed(&mut self) -> bool {
        self.receiver.as_mut().unwrap().changed().await.is_ok()
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
            source: Source(Arc::downgrade(self)),
            command: command.to_owned(),
            run: run.map(str::to_owned),
            receiver: Some(sender.subscribe()),
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

impl Drop for Changes {
    fn drop(&mut self) {
        self.receiver.take();
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
