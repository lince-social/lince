use engine::Engine;
use nucleus::Fact;
use tokio::sync::{broadcast, watch};

pub enum SyncEvent {
    Fact(Fact),
    Refresh,
    Ephemeral,
    Presence,
}

pub struct SyncEvents {
    facts: broadcast::Receiver<Fact>,
    queries: watch::Receiver<u64>,
    ephemeral: tokio::time::Interval,
    presence: Option<watch::Receiver<u64>>,
}

impl SyncEvents {
    pub fn new(engine: &Engine) -> Self {
        let mut ephemeral = tokio::time::interval(std::time::Duration::from_secs(3));
        ephemeral.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        Self {
            facts: engine.subscribe(),
            queries: engine.watch_query_changes(),
            ephemeral,
            presence: None,
        }
    }

    pub fn with_presence(mut self, presence: watch::Receiver<u64>) -> Self {
        self.presence = Some(presence);
        self
    }

    pub async fn next(&mut self, ephemeral: bool) -> Option<SyncEvent> {
        tokio::select! {
            _ = async { self.presence.as_mut().expect("presence receiver").changed().await }, if self.presence.is_some() => Some(SyncEvent::Presence),
            fact = self.facts.recv() => match fact {
                Ok(fact) => Some(SyncEvent::Fact(fact)),
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    self.facts = self.facts.resubscribe();
                    Some(SyncEvent::Refresh)
                }
                Err(broadcast::error::RecvError::Closed) => None,
            },
            changed = self.queries.changed() => changed.ok().map(|()| SyncEvent::Refresh),
            _ = self.ephemeral.tick(), if ephemeral => Some(SyncEvent::Ephemeral),
        }
    }
}
