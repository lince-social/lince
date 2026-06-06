use domain::clean::karma::Karma;
use persistence::repositories::{
    collection::{CollectionRepository, CollectionRepositoryImpl},
    command::{CommandRepository, CommandRepositoryImpl},
    configuration::{ConfigurationRepository, ConfigurationRepositoryImpl},
    frequency::{FrequencyRepository, FrequencyRepositoryImpl},
    karma::{KarmaRepository, KarmaRepositoryImpl},
    operation::{OperationRepository, OperationRepositoryImpl},
    query::{QueryRepository, QueryRepositoryImpl},
    record::{RecordRepository, RecordRepositoryImpl},
    table::{TableRepository, TableRepositoryImpl},
    user::{UserRepository, UserRepositoryImpl},
    view::{ViewRepository, ViewRepositoryImpl},
};
use persistence::storage::StorageService;
use persistence::write_coordinator::WriteCoordinatorHandle;
use serde::Serialize;
use sqlx::{Pool, Sqlite};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, RwLock},
};

pub struct Repositories {
    pub configuration: Arc<dyn ConfigurationRepository>,
    pub operation: Arc<dyn OperationRepository>,
    pub query: Arc<dyn QueryRepository>,
    pub record: Arc<dyn RecordRepository>,
    pub table: Arc<dyn TableRepository>,
    pub command: Arc<dyn CommandRepository>,
    pub frequency: Arc<dyn FrequencyRepository>,
    pub karma: Arc<dyn KarmaRepository>,
    pub collection: Arc<dyn CollectionRepository>,
    pub user: Arc<dyn UserRepository>,
    pub view: Arc<dyn ViewRepository>,
}

pub struct Injected {
    pub db: Arc<Pool<Sqlite>>,
    pub repository: Repositories,
    pub storage: Arc<StorageService>,
    pub writer: WriteCoordinatorHandle,
    pub karma_cache: Arc<KarmaCache>,
    pub file_sync_config: Arc<RwLock<Option<FileSyncConfig>>>,
    pub remote_organ_auth: Arc<RwLock<HashMap<i64, String>>>,
    pub notifications: Arc<NotificationStore>,
}

pub type InjectedServices = Arc<Injected>;

#[derive(Debug, Clone)]
pub struct FileSyncConfig {
    pub enabled: bool,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppNotification {
    pub id: String,
    pub kind: String,
    pub severity: String,
    pub title: String,
    pub body: String,
    pub organ_id: Option<i64>,
    pub created_at_unix: i64,
}

#[derive(Default)]
pub struct NotificationStore {
    notifications: RwLock<Vec<AppNotification>>,
}

impl NotificationStore {
    pub fn upsert(&self, notification: AppNotification) {
        let mut notifications = self
            .notifications
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(existing) = notifications
            .iter_mut()
            .find(|item| item.id == notification.id)
        {
            *existing = notification;
        } else {
            notifications.push(notification);
        }
    }

    pub fn dismiss(&self, notification_id: &str) -> bool {
        let mut notifications = self
            .notifications
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let before = notifications.len();
        notifications.retain(|item| item.id != notification_id);
        before != notifications.len()
    }

    pub fn list(&self) -> Vec<AppNotification> {
        let mut notifications = self
            .notifications
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        notifications.sort_by(|left, right| right.created_at_unix.cmp(&left.created_at_unix));
        notifications
    }
}

#[derive(Default)]
pub struct KarmaCache {
    karma_by_record: RwLock<HashMap<u32, Vec<Karma>>>,
}

impl KarmaCache {
    pub fn replace(&self, karma_by_record: HashMap<u32, Vec<Karma>>) {
        *self
            .karma_by_record
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = karma_by_record;
    }

    pub fn karma_for_record(&self, record_id: u32) -> Vec<Karma> {
        self.karma_by_record
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(&record_id)
            .cloned()
            .unwrap_or_default()
    }
}

pub fn dependency_injection(
    db: Arc<Pool<Sqlite>>,
    storage: Arc<StorageService>,
    writer: WriteCoordinatorHandle,
) -> InjectedServices {
    let services: InjectedServices = Arc::new(Injected {
        db: db.clone(),
        repository: Repositories {
            configuration: Arc::new(ConfigurationRepositoryImpl::new(db.clone())),
            operation: Arc::new(OperationRepositoryImpl::new(db.clone())),
            query: Arc::new(QueryRepositoryImpl::new(db.clone())),
            record: Arc::new(RecordRepositoryImpl::new(db.clone())),
            table: Arc::new(TableRepositoryImpl::new(db.clone())),
            command: Arc::new(CommandRepositoryImpl::new(db.clone())),
            frequency: Arc::new(FrequencyRepositoryImpl::new(db.clone())),
            karma: Arc::new(KarmaRepositoryImpl::new(db.clone())),
            collection: Arc::new(CollectionRepositoryImpl::new(db.clone())),
            user: Arc::new(UserRepositoryImpl::new(db.clone())),
            view: Arc::new(ViewRepositoryImpl::new(db.clone())),
        },
        storage,
        writer,
        karma_cache: Arc::new(KarmaCache::default()),
        file_sync_config: Arc::new(RwLock::new(None)),
        remote_organ_auth: Arc::new(RwLock::new(HashMap::new())),
        notifications: Arc::new(NotificationStore::default()),
    });

    services
}
