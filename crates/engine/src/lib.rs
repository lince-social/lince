#![recursion_limit = "256"]

pub mod access;
pub mod action_intent;
pub mod actions;
pub mod area_transition;
pub mod append;
pub mod body_links;
pub mod checkpoint;
pub mod collab;
pub mod collab_guard;
#[allow(dead_code)]
pub mod communication;
pub mod directory;
pub mod effects;
pub mod enrolment;
pub mod error;
pub mod expiry;
pub mod file_sync;
pub mod imagination;
pub mod instinct;
pub mod karma_control;
pub mod karma_grants;
pub mod karma_runtime;
pub mod karma_timezone;
pub mod lingua_file;
pub mod mailbox;
pub mod pairing;
pub mod peers;
pub mod private_admin_catalog;
pub mod private_auth;
pub mod private_files;
pub mod private_json;
pub mod private_password;
pub mod private_requests;
pub mod private_work;
pub mod read_filter;
mod tagged_record;
pub mod rebuild;
pub mod roster;
pub mod seal;
pub mod senses;
pub mod share;
pub mod signals;
pub mod sync;
pub mod threads;
pub mod transfer;
pub mod transfer_delivery;
pub mod trust;
pub mod wire;

use chrono::{DateTime, TimeDelta, Utc};

use crate::actions::Action;

const RECURRENCE_CATCH_UP_DAYS: i64 = 60;

const READ_MODEL_AUDIT_HOURS: i64 = 6;

const ROSTER_GRACE_DAYS: i64 = 7;

const MAX_AUTO_APPLIES_PER_RULE_PER_TICK: usize = 64;

const MAX_REACTIONS_PER_CHANGE: usize = 256;

pub(crate) fn as_one_firing<F>(work: F) -> impl std::future::Future<Output = F::Output>
where
    F: std::future::Future,
{
    INSIDE_REACTION.scope((), work)
}

pub(crate) fn already_firing() -> bool {
    INSIDE_REACTION.try_with(|()| ()).is_ok()
}

tokio::task_local! {
    static INSIDE_REACTION: ();
}
use nucleus::{Cause, Fact, NewFact};
use std::sync::RwLock;
use store::Store;
use tokio::sync::{Mutex, broadcast, watch};

pub use error::EngineError;

pub struct Engine {
    pub store: Store,
    bus: broadcast::Sender<Fact>,
    query_changed: watch::Sender<u64>,
    pub(crate) signer: Mutex<Option<trust::Signer>>,
    pub(crate) organ_signer: Mutex<Option<trust::Signer>>,
    karma_deadline_changed: watch::Sender<u64>,
    notifications_changed: watch::Sender<u64>,
    config_changed: watch::Sender<u64>,
    karma_runtime_config: RwLock<Option<karma_runtime::KarmaDeadlineDirectorConfig>>,
    pub(crate) collab_docs: std::sync::Mutex<collab::DocRegistry>,
    pub(crate) root_key_path: std::sync::Mutex<Option<std::path::PathBuf>>,
    pub(crate) sealing_keyring_path: std::sync::Mutex<Option<std::path::PathBuf>>,
    pub(crate) nearby: std::sync::Mutex<Option<wire::Nearby>>,
    pub(crate) file_sync_conflicts:
        std::sync::Mutex<std::collections::HashMap<String, Vec<file_sync::FileConflict>>>,
    pub(crate) import_lock: tokio::sync::Mutex<()>,
    pub(crate) directory: tokio::sync::OnceCell<directory::Directory>,
    pub(crate) joining: std::sync::atomic::AtomicBool,
    pub(crate) enroller: std::sync::Mutex<Option<std::sync::Weak<dyn enrolment::CellTransport>>>,
    pub(crate) stale_siblings: std::sync::Mutex<Vec<wire::StaleSibling>>,
}

impl Engine {
    pub fn attach_nearby(&self, nearby: wire::Nearby) {
        *self.nearby.lock().expect("nearby handle") = Some(nearby);
    }

    pub fn nearby_peers(&self) -> Vec<wire::NearbyPeer> {
        self.nearby
            .lock()
            .expect("nearby handle")
            .as_ref()
            .map(|nearby| nearby.current())
            .unwrap_or_default()
    }

    pub async fn new(store: Store) -> Result<Engine, EngineError> {
        if let Some(max) = store::sync_ops::max_hlc(&store.pool).await? {
            nucleus::hlc::observe(max);
        }
        let (bus, _) = broadcast::channel(1024);
        let (query_changed, _) = watch::channel(0);
        let (karma_deadline_changed, _) = watch::channel(0);
        let (notifications_changed, _) = watch::channel(0);
        let (config_changed, _) = watch::channel(0);
        let engine = Engine {
            store,
            bus,
            query_changed,
            signer: Mutex::new(None),
            organ_signer: Mutex::new(None),
            karma_deadline_changed,
            notifications_changed,
            config_changed,
            karma_runtime_config: RwLock::new(None),
            collab_docs: std::sync::Mutex::new(collab::DocRegistry::default()),
            root_key_path: std::sync::Mutex::new(None),
            sealing_keyring_path: std::sync::Mutex::new(None),
            nearby: std::sync::Mutex::new(None),
            file_sync_conflicts: std::sync::Mutex::new(std::collections::HashMap::new()),
            import_lock: tokio::sync::Mutex::new(()),
            directory: tokio::sync::OnceCell::new(),
            joining: std::sync::atomic::AtomicBool::new(false),
            enroller: std::sync::Mutex::new(None),
            stale_siblings: std::sync::Mutex::new(Vec::new()),
        };
        Ok(engine)
    }

    pub async fn open_memory() -> Result<Engine, EngineError> {
        Self::new(Store::open_memory().await?).await
    }

    pub async fn open(url: &str) -> Result<Engine, EngineError> {
        Self::new(Store::open(url).await?).await
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Fact> {
        self.bus.subscribe()
    }

    pub fn watch_query_changes(&self) -> watch::Receiver<u64> {
        self.query_changed.subscribe()
    }

    pub fn watch_notifications(&self) -> watch::Receiver<u64> {
        self.notifications_changed.subscribe()
    }

    pub fn watch_config(&self) -> watch::Receiver<u64> {
        self.config_changed.subscribe()
    }

    pub fn notify_config_changed(&self) {
        self.config_changed
            .send_modify(|revision| *revision = revision.wrapping_add(1));
    }

    pub fn notify_notifications_changed(&self) {
        self.notifications_changed
            .send_modify(|revision| *revision = revision.wrapping_add(1));
    }

    pub async fn notifications(&self) -> Result<Vec<serde_json::Value>, EngineError> {
        Ok(store::invites::pending(&self.store.pool)
            .await?
            .into_iter()
            .map(|invite| {
                serde_json::json!({
                    "id": invite.record_uid,
                    "kind": "thread_invite",
                    "title": "Conversation request",
                    "body": format!(
                        "{} wants to start an individual synced conversation.",
                        invite.from_organ
                    ),
                    "recordId": invite.root,
                    "organId": invite.from_organ,
                })
            })
            .collect())
    }

    pub fn notify_karma_deadline_change(&self) {
        self.karma_deadline_changed
            .send_modify(|revision| *revision = revision.wrapping_add(1));
    }

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

    pub async fn append(&self, new: NewFact, now: DateTime<Utc>) -> Result<Vec<Fact>, EngineError> {
        let signer = self.signer.lock().await.clone();
        if let Some(fact) = append::append_one(&self.store, new, now, signer.as_ref()).await? {
            return self.observe_committed_fact(fact, now).await;
        }
        Ok(Vec::new())
    }

    pub(crate) async fn observe_committed_fact(
        &self,
        fact: Fact,
        now: DateTime<Utc>,
    ) -> Result<Vec<Fact>, EngineError> {
        let _ = self.bus.send(fact.clone());
        let changed = vec![fact.record_uid.clone()];
        let mut committed = vec![fact];
        committed.extend(self.react_to(changed, now).await?);
        Ok(committed)
    }

    pub(crate) async fn react_to(
        &self,
        changed: Vec<String>,
        now: DateTime<Utc>,
    ) -> Result<Vec<Fact>, EngineError> {
        if already_firing() {
            return Ok(Vec::new());
        }
        as_one_firing(self.react_to_inner(changed, now)).await
    }

    async fn react_to_inner(
        &self,
        changed: Vec<String>,
        now: DateTime<Utc>,
    ) -> Result<Vec<Fact>, EngineError> {
        let mut committed = Vec::new();
        let watchers: Vec<store::recurrence::Recurrence> = store::recurrence::all(&self.store.pool)
            .await?
            .into_iter()
            .filter(|rule| rule.condition.is_some() && !rule.is_paused())
            .collect();
        if watchers.is_empty() {
            return Ok(committed);
        }

        let mut pending: std::collections::VecDeque<String> = changed.into();
        let mut steps = 0usize;
        while let Some(record_uid) = pending.pop_front() {
            for rule in &watchers {
                steps += 1;
                if steps > MAX_REACTIONS_PER_CHANGE {
                    return Ok(committed);
                }
                if !self.rule_reads(rule, &record_uid).await? {
                    continue;
                }
                let Ok(anchor) = actions::parse_instant_field(&rule.anchor_at) else {
                    continue;
                };
                let Some(edge) = now.checked_add_signed(TimeDelta::milliseconds(1)) else {
                    continue;
                };
                let Ok(Some(due)) = rule.cadence.preceding(anchor, edge) else {
                    continue;
                };
                let outcome = Box::pin(self.act_at(
                    Action::ApplyRecurrenceOccurrence {
                        recurrence: rule.uid.clone(),
                        due_at: due.to_rfc3339(),
                        amount: None,
                        note: None,
                    },
                    None,
                    now,
                ))
                .await;
                let Ok(outcome) = outcome else { continue };
                for fact in &outcome.facts {
                    let _ = self.bus.send(fact.clone());
                    pending.push_back(fact.record_uid.clone());
                }
                committed.extend(outcome.facts);
            }
        }
        Ok(committed)
    }

    async fn rule_reads(
        &self,
        rule: &store::recurrence::Recurrence,
        record_uid: &str,
    ) -> Result<bool, EngineError> {
        let Some(condition) = rule.condition.as_ref() else {
            return Ok(false);
        };
        let Ok(parsed) = nucleus::karma::Condition::parse(&condition.source) else {
            return Ok(false);
        };
        for token in parsed.reads() {
            if token.func == "freq" {
                continue;
            }
            let name = token.slug.trim_start_matches('@');
            if let Some(record) = store::records::resolve(&self.store.pool, name).await?
                && record.uid == record_uid
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub(crate) fn publish_committed_fact(&self, fact: Fact) -> Vec<Fact> {
        let _ = self.bus.send(fact.clone());
        vec![fact]
    }

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

    pub async fn fire_due_rules(&self, now: DateTime<Utc>) -> Result<Vec<Fact>, EngineError> {
        let facts = self.fire_due_rules_inner(now).await?;
        let mut changed: Vec<String> = facts.iter().map(|f| f.record_uid.clone()).collect();
        changed.sort();
        changed.dedup();
        let mut committed = facts;
        committed.extend(self.react_to(changed, now).await?);
        Ok(committed)
    }

    async fn fire_due_rules_inner(&self, now: DateTime<Utc>) -> Result<Vec<Fact>, EngineError> {
        let mut committed = Vec::new();
        let from = now - TimeDelta::days(RECURRENCE_CATCH_UP_DAYS);
        let Some(to) = now.checked_add_signed(TimeDelta::milliseconds(1)) else {
            return Ok(committed);
        };
        for rule in store::recurrence::all(&self.store.pool).await? {
            if rule.is_paused() {
                continue;
            }
            let Ok(derived) =
                store::recurrence::occurrences(&self.store.pool, &rule, from, to, now).await
            else {
                continue;
            };
            let mut applied = 0usize;
            for occurrence in derived.dates {
                if occurrence.state != store::recurrence::OccurrenceState::Due {
                    continue;
                }
                if applied >= MAX_AUTO_APPLIES_PER_RULE_PER_TICK {
                    break;
                }
                applied += 1;
                let outcome = self
                    .act_at(
                        Action::ApplyRecurrenceOccurrence {
                            recurrence: rule.uid.clone(),
                            due_at: occurrence.due_at.to_rfc3339(),
                            amount: None,
                            note: None,
                        },
                        None,
                        now,
                    )
                    .await;
                match outcome {
                    Ok(outcome) => {
                        for fact in &outcome.facts {
                            let _ = self.bus.send(fact.clone());
                        }
                        committed.extend(outcome.facts);
                    }
                    Err(_) => continue,
                }
            }
        }
        Ok(committed)
    }

    pub async fn heartbeat(&self, now: DateTime<Utc>) -> Result<Vec<Fact>, EngineError> {
        let mut facts = self.expire_promises(now).await?;
        facts.extend(self.expire_due_transfer_invitations(now).await?);
        facts.extend(self.expire_decisions(now).await?);
        facts.extend(self.fire_due_rules(now).await?);
        facts.extend(self.sample_due_signals(now).await?);
        self.run_due_effects().await?;
        self.senses_pass().await?;
        self.crossings_pass(now).await?;
        self.audit_read_model_if_due(now, chrono::Duration::hours(READ_MODEL_AUDIT_HOURS))
            .await?;
        Ok(facts)
    }

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
