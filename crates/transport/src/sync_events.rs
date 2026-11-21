//! Merges fact, query, clock, and presence updates for a live session.

use std::{fmt, time::Duration};

use engine::{Engine, QueryChanges};
use nucleus::Fact;
use tokio::{
    sync::{
        broadcast,
        broadcast::error::{RecvError, TryRecvError},
        watch,
    },
    time::{Interval, MissedTickBehavior, interval},
};

/// Work available for the live session.
#[allow(
    clippy::large_enum_variant,
    reason = "the public Fact variant carries one fact inline without allocating"
)]
pub enum SyncEvent {
    /// A single new fact.
    Fact(Fact),
    /// Several consecutive facts in order.
    Facts(Vec<Fact>),
    /// A full refresh after a general change or missed facts.
    Refresh,
    /// A projection-only refresh.
    Projection,
    /// The periodic ephemeral tick.
    Ephemeral,
    /// A presence update.
    Presence,
}

impl fmt::Debug for SyncEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Fact(_) => "Fact(..)",
            Self::Facts(_) => "Facts(..)",
            Self::Refresh => "Refresh",
            Self::Projection => "Projection",
            Self::Ephemeral => "Ephemeral",
            Self::Presence => "Presence",
        })
    }
}

const MAX_FACT_BATCH: usize = 64;

/// A live session's independent event receivers and query cursor.
pub struct SyncEvents {
    facts: broadcast::Receiver<Fact>,
    queries: watch::Receiver<QueryChanges>,
    observed_queries: Option<QueryChanges>,
    ephemeral: Interval,
    presence: Option<watch::Receiver<u64>>,
}

impl fmt::Debug for SyncEvents {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SyncEvents")
            .field("observed_queries", &self.observed_queries)
            .field("has_presence", &self.presence.is_some())
            .finish_non_exhaustive()
    }
}

impl SyncEvents {
    /// Subscribes to the Engine's live events.
    ///
    /// # Panics
    /// Requires a Tokio runtime with time enabled.
    pub fn new(engine: &Engine) -> Self {
        Self::from_receivers(engine.subscribe(), engine.watch_query_changes())
    }

    fn from_receivers(
        facts: broadcast::Receiver<Fact>,
        queries: watch::Receiver<QueryChanges>,
    ) -> Self {
        let mut ephemeral = interval(Duration::from_secs(3));
        ephemeral.set_missed_tick_behavior(MissedTickBehavior::Delay);
        let current = *queries.borrow();
        let observed_queries = matches!(queries.has_changed(), Ok(false)).then_some(current);
        Self {
            facts,
            queries,
            observed_queries,
            ephemeral,
            presence: None,
        }
    }

    /// Adds a source of presence updates.
    pub fn with_presence(mut self, presence: watch::Receiver<u64>) -> Self {
        self.presence = Some(presence);
        self
    }

    /// Waits for the next event. `ephemeral` enables the periodic tick.
    pub async fn next(&mut self, ephemeral: bool) -> Option<SyncEvent> {
        tokio::select! {
            // An update or closure on an attached watcher schedules a presence refresh.
            Some(_) = async { Some(self.presence.as_mut()?.changed().await) } => Some(SyncEvent::Presence),
            fact = self.facts.recv() => match fact {
                Ok(fact) => Some(self.fact_batch(fact)),
                Err(RecvError::Lagged(_)) => Some(self.refresh_facts()),
                Err(RecvError::Closed) => None,
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
            Err(TryRecvError::Empty | TryRecvError::Closed) => return SyncEvent::Fact(first),
            Err(TryRecvError::Lagged(_)) => return self.refresh_facts(),
        };
        let mut facts = vec![first, second];
        while facts.len() < MAX_FACT_BATCH {
            match self.facts.try_recv() {
                Ok(fact) => facts.push(fact),
                Err(TryRecvError::Empty | TryRecvError::Closed) => break,
                Err(TryRecvError::Lagged(_)) => return self.refresh_facts(),
            }
        }
        SyncEvent::Facts(facts)
    }

    fn refresh_facts(&mut self) -> SyncEvent {
        self.facts = self.facts.resubscribe();
        SyncEvent::Refresh
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nucleus::{Cause, NewFact, execution::now, fact::seal};

    fn fixture_fact(sequence: usize) -> Fact {
        let mut draft = NewFact::quantity_f64("fixture-record", 0.0, Cause::user_edit());
        draft.uid = Some(sequence.to_string());
        seal(draft, "genesis", now())
    }

    #[tokio::test]
    async fn fact_batches_keep_order_and_recover_from_lag() {
        let (sender, facts) = broadcast::channel(MAX_FACT_BATCH * 2);
        let (changes, queries) = watch::channel(QueryChanges::default());
        let mut events = SyncEvents::from_receivers(facts, queries);
        for sequence in 0..=MAX_FACT_BATCH {
            sender
                .send(fixture_fact(sequence))
                .expect("a live subscriber");
        }
        let Some(SyncEvent::Facts(batch)) = events.next(false).await else {
            panic!("expected a full fact batch");
        };
        assert_eq!(batch.len(), MAX_FACT_BATCH);
        for (sequence, fact) in batch.iter().enumerate() {
            assert_eq!(fact.uid, sequence.to_string());
        }
        assert!(
            matches!(events.next(false).await, Some(SyncEvent::Fact(fact)) if fact.uid == MAX_FACT_BATCH.to_string())
        );

        let (sender, facts) = broadcast::channel(2);
        let mut events = SyncEvents::from_receivers(facts, changes.subscribe());
        for sequence in 0..3 {
            sender
                .send(fixture_fact(sequence))
                .expect("a live subscriber");
        }
        assert!(matches!(events.next(false).await, Some(SyncEvent::Refresh)));
        sender.send(fixture_fact(3)).expect("a live subscriber");
        assert!(matches!(events.next(false).await, Some(SyncEvent::Fact(fact)) if fact.uid == "3"));
    }

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
