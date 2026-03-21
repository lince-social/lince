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
use sqlx::{Pool, Sqlite};
use std::sync::Arc;

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
}

pub type InjectedServices = Arc<Injected>;

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
    });

    services
}
