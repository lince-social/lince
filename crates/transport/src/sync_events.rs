use engine::Engine;
use nucleus::Fact;
use tokio::sync::{broadcast, watch};

pub enum SyncEvent {
    Fact(Fact),
    Facts(Vec<Fact>),
    Refresh,
    Projection,
    Ephemeral,
    Presence,
}

const MAX_FACT_BATCH: usize = 64;

pub struct SyncEvents {
    facts: broadcast::Receiver<Fact>,
    queries: watch::Receiver<engine::QueryChanges>,
    observed_queries: Option<engine::QueryChanges>,
    ephemeral: tokio::time::Interval,
    presence: Option<watch::Receiver<u64>>,
}

impl SyncEvents {
    pub fn new(engine: &Engine) -> Self {
        Self::from_receivers(engine.subscribe(), engine.watch_query_changes())
    }

    fn from_receivers(
        facts: broadcast::Receiver<Fact>,
        queries: watch::Receiver<engine::QueryChanges>,
    ) -> Self {
        let mut ephemeral = tokio::time::interval(std::time::Duration::from_secs(3));
        ephemeral.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let current = *queries.borrow();
        let observed_queries = match queries.has_changed() {
            Ok(false) => Some(current),
            _ => None,
        };
        Self {
            facts,
            queries,
            observed_queries,
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
                Ok(fact) => Some(self.fact_batch(fact)),
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    self.facts = self.facts.resubscribe();
                    Some(SyncEvent::Refresh)
                }
                Err(broadcast::error::RecvError::Closed) => None,
            },
            changed = self.queries.changed() => changed.ok().map(|()| {
                let current = *self.queries.borrow_and_update();
                let event = if self.observed_queries.is_none_or(|previous| current.general != previous.general) {
                    SyncEvent::Refresh
                } else {
                    SyncEvent::Projection
                };
                self.observed_queries = Some(current);
                event
            }),
            _ = self.ephemeral.tick(), if ephemeral => Some(SyncEvent::Ephemeral),
        }
    }

    fn fact_batch(&mut self, first: Fact) -> SyncEvent {
        let second = match self.facts.try_recv() {
            Ok(fact) => fact,
            Err(broadcast::error::TryRecvError::Empty | broadcast::error::TryRecvError::Closed) => {
                return SyncEvent::Fact(first);
            }
            Err(broadcast::error::TryRecvError::Lagged(_)) => {
                self.facts = self.facts.resubscribe();
                return SyncEvent::Refresh;
            }
        };
        let mut facts = vec![first, second];
        while facts.len() < MAX_FACT_BATCH {
            match self.facts.try_recv() {
                Ok(fact) => facts.push(fact),
                Err(
                    broadcast::error::TryRecvError::Empty | broadcast::error::TryRecvError::Closed,
                ) => {
                    break;
                }
                Err(broadcast::error::TryRecvError::Lagged(_)) => {
                    self.facts = self.facts.resubscribe();
                    return SyncEvent::Refresh;
                }
            }
        }
        SyncEvent::Facts(facts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn coalesced_general_changes_cannot_be_hidden_by_forecast_notifications() {
        let (_facts, facts) = broadcast::channel(1);
        let (changed, queries) = watch::channel(engine::QueryChanges::default());
        let mut events = SyncEvents::from_receivers(facts, queries);
        for general_first in [true, false] {
            if general_first {
                changed.send_modify(|state| state.general += 1);
            }
            for _ in 0..128 {
                changed.send_modify(|state| state.projection += 1);
            }
            if !general_first {
                changed.send_modify(|state| state.general += 1);
            }
            assert!(matches!(events.next(false).await, Some(SyncEvent::Refresh)));
            assert!(!events.queries.has_changed().unwrap());
            changed.send_modify(|state| state.projection += 1);
            assert!(matches!(
                events.next(false).await,
                Some(SyncEvent::Projection)
            ));
            assert!(!events.queries.has_changed().unwrap());
            changed.send_modify(|state| state.general += 1);
            assert!(matches!(events.next(false).await, Some(SyncEvent::Refresh)));
        }
    }

    #[tokio::test]
    async fn a_change_arriving_before_watcher_initialization_is_not_consumed_or_narrowed() {
        let (_facts, facts) = broadcast::channel(1);
        let (changed, queries) = watch::channel(engine::QueryChanges::default());
        changed.send_modify(|state| state.general += 1);
        let mut events = SyncEvents::from_receivers(facts, queries);
        let event = tokio::time::timeout(std::time::Duration::from_secs(1), events.next(false))
            .await
            .unwrap();
        assert!(matches!(event, Some(SyncEvent::Refresh)));
        changed.send_modify(|state| state.projection += 1);
        assert!(matches!(
            events.next(false).await,
            Some(SyncEvent::Projection)
        ));
        assert_eq!(events.queries.borrow().revision(), 2);
    }
}
