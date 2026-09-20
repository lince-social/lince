#![recursion_limit = "256"]

mod fiote_config;

pub mod access;
pub mod action_intent;
pub mod actions;
pub mod append;
pub mod area_transition;
pub mod body_links;
pub mod checkpoint;
pub mod collab;
pub mod collab_guard;
pub mod presence;
pub mod record_change;
pub mod record_creation;
#[allow(dead_code)]
pub mod communication;
pub mod directory;
pub mod effects;
pub mod enrolment;
pub mod error;
pub mod expiry;
pub mod file_sync;
pub mod operation_origin;
pub mod imagination;
pub mod instinct;
pub mod karma_control;
pub mod karma_grants;
pub mod karma_runtime;
pub mod rule_runtime;
pub mod karma_timezone;
pub mod lingua_file;
pub mod login;
mod role_management;
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
pub mod rebuild;
pub mod roster;
pub mod seal;
pub mod senses;
pub mod share;
pub mod signals;
pub mod sync;
pub mod sync_service;
mod tagged_record;
pub mod threads;
pub mod transfer;
pub mod transfer_delivery;
pub mod trust;
pub mod wire;

use chrono::{DateTime, Utc};

const READ_MODEL_AUDIT_HOURS: i64 = 6;

const ROSTER_GRACE_DAYS: i64 = 7;

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
    pub passwords: private_password::PasswordWork,
    access_gate: tokio::sync::RwLock<()>,
    fiote_config_lock: Mutex<()>,
    thread_creation_lock: Mutex<()>,
    login_attempts: tokio::sync::Mutex<login::LoginAttempts>,
    pub sync_service: sync_service::SyncService,
    pub presence: presence::Presence,
    bus: broadcast::Sender<Fact>,
    query_changed: watch::Sender<u64>,
    pub(crate) signer: Mutex<Option<trust::Signer>>,
    pub(crate) organ_signer: Mutex<Option<trust::Signer>>,
    karma_deadline_changed: watch::Sender<u64>,
    notifications_changed: watch::Sender<u64>,
    config_changed: watch::Sender<u64>,
    karma_runtime_config: RwLock<Option<karma_runtime::KarmaDeadlineDirectorConfig>>,
    pub(crate) rule_index: Mutex<Option<(u64, std::sync::Arc<rule_runtime::RuleIndex>)>>,
    pub(crate) rule_execution: Mutex<()>,
    pub(crate) effects_changed: watch::Sender<u64>,
    pub(crate) effect_execution: Mutex<()>,
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
        let (effects_changed, _) = watch::channel(0);
        let engine = Engine {
            store,
            passwords: private_password::PasswordWork::new(2)
                .map_err(|error| EngineError::Consequence(error.to_string()))?,
            fiote_config_lock: Mutex::new(()),
            thread_creation_lock: Mutex::new(()),
            access_gate: tokio::sync::RwLock::new(()),
            login_attempts: tokio::sync::Mutex::new(login::LoginAttempts::default()),
            sync_service: sync_service::SyncService::default(),
            presence: presence::Presence::default(),
            bus,
            query_changed,
            signer: Mutex::new(None),
            organ_signer: Mutex::new(None),
            karma_deadline_changed,
            notifications_changed,
            config_changed,
            karma_runtime_config: RwLock::new(None),
            rule_index: Mutex::new(None),
            rule_execution: Mutex::new(()),
            effects_changed,
            effect_execution: Mutex::new(()),
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
        let event_id = fact.uid.clone();
        let mut committed = vec![fact];
        committed.extend(Box::pin(self.react_to_event(changed, event_id, now)).await?);
        Ok(committed)
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
        self.advance_karma_time(now).await
    }

    pub async fn heartbeat(&self, now: DateTime<Utc>) -> Result<Vec<Fact>, EngineError> {
        if let Err(error) = store::sync_activity::prune(&self.store.pool, now.timestamp()).await {
            tracing::warn!(%error, "Could not expire sync history");
        }
        let mut facts = self.expire_promises(now).await?;
        facts.extend(self.expire_due_transfer_invitations(now).await?);
        facts.extend(self.expire_decisions(now).await?);
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
