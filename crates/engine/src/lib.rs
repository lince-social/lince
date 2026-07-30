//! Lince engine (blueprint Part 0): one organism, one write path.
//!
//! - `append`/`append_user`: the ONLY code that mutates `record.quantity`
//!   (fact + cache bump in one transaction), followed by a synchronous,
//!   iteration-capped Karma cascade — deterministic and DST-friendly.
//! - `tick(now)`: the timer wheel — fires due Frequencies into rules that read
//!   them via `freq(@slug)`.
//! - `run_due_effects`: executes queued shell/notify effects OUTSIDE evaluation.
//! - `reload_rules`: rebuilds the in-memory rule registry + dependency graph,
//!   returning Proof warnings (rule loops).

pub mod action_intent;
pub mod actions;
pub mod append;
pub mod checkpoint;
#[allow(dead_code)]
pub mod communication;
pub mod effects;
pub mod error;
pub mod expiry;
pub mod file_sync;
pub mod imagination;
pub mod karma;
pub mod karma_control;
pub mod karma_grants;
pub mod karma_runtime;
pub mod karma_timezone;
pub mod senses;
pub mod signals;
pub mod sync;
pub mod transfer;
pub mod transfer_delivery;
pub mod trust;

use chrono::{DateTime, Utc};
use nucleus::{Cause, Fact, NewFact};
use std::sync::RwLock;
use store::Store;
use tokio::sync::{Mutex, broadcast, watch};

pub use error::EngineError;
pub use karma::ProofWarning;

pub struct Engine {
    pub store: Store,
    registry: Mutex<karma::Registry>,
    bus: broadcast::Sender<Fact>,
    pub(crate) signer: Mutex<Option<trust::Signer>>,
    pub(crate) organ_signer: Mutex<Option<trust::Signer>>,
    karma_deadline_changed: watch::Sender<u64>,
    karma_runtime_config: RwLock<Option<karma_runtime::KarmaDeadlineDirectorConfig>>,
}

impl Engine {
    pub async fn new(store: Store) -> Result<Engine, EngineError> {
        let (bus, _) = broadcast::channel(1024);
        let (karma_deadline_changed, _) = watch::channel(0);
        let engine = Engine {
            store,
            registry: Mutex::new(karma::Registry::default()),
            bus,
            signer: Mutex::new(None),
            organ_signer: Mutex::new(None),
            karma_deadline_changed,
            karma_runtime_config: RwLock::new(None),
        };
        engine.reload_rules().await?;
        Ok(engine)
    }

    /// A clone of the current rule registry (Imagination builds snapshots from it).
    pub(crate) async fn registry_snapshot(&self) -> karma::Registry {
        self.registry.lock().await.clone()
    }

    pub async fn open_memory() -> Result<Engine, EngineError> {
        Self::new(Store::open_memory().await?).await
    }

    /// Subscribe to committed facts (blueprint 0.2 `fact_bus`).
    pub fn subscribe(&self) -> broadcast::Receiver<Fact> {
        self.bus.subscribe()
    }

    /// Wake the tickless Karma deadline runner after a committed activation,
    /// pause, parameter epoch, resource grant, or host timer capability change.
    pub fn notify_karma_deadline_change(&self) {
        self.karma_deadline_changed
            .send_modify(|revision| *revision = revision.wrapping_add(1));
    }

    /// Install the immutable admission/provider snapshot used by typed Karma
    /// Frequency Actions. Starting the director and installing its control
    /// snapshot are explicit so boot code can finish dependency injection
    /// before either mutations or timers are accepted.
    pub fn install_karma_runtime_config(
        &self,
        config: karma_runtime::KarmaDeadlineDirectorConfig,
    ) -> Result<(), EngineError> {
        *self
            .karma_runtime_config
            .write()
            .map_err(|_| EngineError::Conflict {
                code: "karma_runtime_config_poisoned",
                message: "Karma runtime configuration lock is poisoned".to_string(),
            })? = Some(config);
        self.notify_karma_deadline_change();
        Ok(())
    }

    pub(crate) fn configured_karma_runtime(
        &self,
    ) -> Result<karma_runtime::KarmaDeadlineDirectorConfig, EngineError> {
        self.karma_runtime_config
            .read()
            .map_err(|_| EngineError::Conflict {
                code: "karma_runtime_config_poisoned",
                message: "Karma runtime configuration lock is poisoned".to_string(),
            })?
            .clone()
            .ok_or_else(|| EngineError::Conflict {
                code: "karma_runtime_unconfigured",
                message: "Karma Frequency activation requires an installed runtime configuration"
                    .to_string(),
            })
    }

    /// Rebuild the rule registry and dependency graph. Returns Proof warnings.
    pub async fn reload_rules(&self) -> Result<Vec<ProofWarning>, EngineError> {
        let registry = karma::Registry::load(&self.store).await?;
        let warnings = registry.proof_warnings.clone();
        *self.registry.lock().await = registry;
        Ok(warnings)
    }

    /// Append a fact and run the reactive Karma cascade. Returns every fact
    /// committed (the trigger plus all rule firings), in order. Every fact is
    /// signed when a signer is installed (Trust, XI).
    pub async fn append(&self, new: NewFact, now: DateTime<Utc>) -> Result<Vec<Fact>, EngineError> {
        let signer = self.signer.lock().await.clone();
        if let Some(fact) = append::append_one(&self.store, new, now, signer.as_ref()).await? {
            return self.observe_committed_fact(fact, now).await;
        }
        Ok(Vec::new())
    }

    /// Publish a Fact that was committed inside a larger semantic transaction,
    /// then run the same reactive cascade as the normal append path.
    pub(crate) async fn observe_committed_fact(
        &self,
        fact: Fact,
        now: DateTime<Utc>,
    ) -> Result<Vec<Fact>, EngineError> {
        let signer = self.signer.lock().await.clone();
        let _ = self.bus.send(fact.clone());
        let changed = vec![fact.record_uid.clone()];
        let mut committed = vec![fact];
        let registry = self.registry.lock().await;
        let cascade = karma::cascade(
            &self.store,
            &registry,
            changed,
            &Default::default(),
            now,
            signer.as_ref(),
        )
        .await?;
        for fact in &cascade {
            let _ = self.bus.send(fact.clone());
        }
        committed.extend(cascade);
        Ok(committed)
    }

    /// Publish semantic transaction evidence without running Karma. Transfer
    /// revisions use this until the dedicated Transfer/Karma phase defines
    /// which signed term changes may drive recommendations or automation.
    pub(crate) fn publish_committed_fact(&self, fact: Fact) -> Vec<Fact> {
        let _ = self.bus.send(fact.clone());
        vec![fact]
    }

    /// Convenience: user edits a quantity by delta, clocked now.
    pub async fn append_user(
        &self,
        record_uid: &str,
        delta: f64,
    ) -> Result<Vec<Fact>, EngineError> {
        self.append(
            NewFact::quantity_f64(record_uid, delta, Cause::user_edit()),
            Utc::now(),
        )
        .await
    }

    /// The timer wheel: fire every due Frequency, evaluate rules reading it.
    pub async fn tick(&self, now: DateTime<Utc>) -> Result<Vec<Fact>, EngineError> {
        let due = store::freqs::due(&self.store.pool, now).await?;
        let signer = self.signer.lock().await.clone();
        let registry = self.registry.lock().await;
        let mut committed = Vec::new();
        for freq in due {
            let Some((periods, next_at)) = freq.spec.fire(now) else {
                continue;
            };
            store::freqs::set_next_at(&self.store.pool, &freq.record_uid, next_at).await?;
            if periods == 0.0 {
                continue; // day_of_week skipped this boundary
            }
            let mut injected = karma::Injected::default();
            injected.freq.insert(freq.record_uid.clone(), periods);
            let cascade = karma::cascade(
                &self.store,
                &registry,
                vec![freq.record_uid.clone()],
                &injected,
                now,
                signer.as_ref(),
            )
            .await?;
            for f in &cascade {
                let _ = self.bus.send(f.clone());
            }
            committed.extend(cascade);
        }
        Ok(committed)
    }

    /// Run queued effects (blueprint VI: Effects never run inside evaluation).
    /// One beat of the organism (blueprint 0.2, callable form): promises
    /// expire, timers fire, signals sample, effects run. The daemon wraps this
    /// in an interval; DST calls it with a virtual clock.
    pub async fn heartbeat(&self, now: DateTime<Utc>) -> Result<Vec<Fact>, EngineError> {
        // Expiry first: rules evaluated by the tick must see true promise states.
        let mut facts = self.expire_promises(now).await?;
        facts.extend(self.expire_due_transfer_invitations(now).await?);
        facts.extend(self.expire_decisions(now).await?);
        facts.extend(self.tick(now).await?);
        facts.extend(self.sample_due_signals(now).await?);
        self.run_due_effects().await?;
        // Senses (X): match open promises against the discovery cache; drafts
        // land in the Decision Queue.
        self.senses_pass().await?;
        // Imagination (XII): projected threshold crossings become decisions.
        self.crossings_pass(now).await?;
        Ok(facts)
    }

    /// The daemon loop: heartbeat every `period_secs` until the handle drops.
    pub fn run(self: std::sync::Arc<Self>, period_secs: u64) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut interval =
                tokio::time::interval(std::time::Duration::from_secs(period_secs.max(1)));
            loop {
                interval.tick().await;
                let _ = self.heartbeat(Utc::now()).await;
            }
        })
    }
}
