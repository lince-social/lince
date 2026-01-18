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
};
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
}

pub struct Injected {
    pub repository: Repositories,
}

pub type InjectedServices = Arc<Injected>;

pub fn dependency_injection(db: Arc<Pool<Sqlite>>) -> InjectedServices {
    let services: InjectedServices = Arc::new(Injected {
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
        },
    });

    services
}
