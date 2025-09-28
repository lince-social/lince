use crate::{
    application::use_cases::{
        configuration::get_active_colorscheme::UseCaseConfigurationGetActiveColorscheme,
        operation::only_digits::UseCaseOnlyDigits,
    },
    domain::repositories::{
        collection::CollectionRepository, command::CommandRepository,
        configuration::ConfigurationRepository, frequency::FrequencyRepository,
        karma::KarmaRepository, operation::OperationRepository, query::QueryRepository,
        record::RecordRepository, table::TableRepository, view::ViewRepository,
    },
    infrastructure::database::repositories::{
        collection::CollectionRepositoryImpl, command::CommandRepositoryImpl,
        configuration::ConfigurationRepositoryImpl, frequency::FrequencyRepositoryImpl,
        karma::KarmaRepositoryImpl, operation::OperationRepositoryImpl, query::QueryRepositoryImpl,
        record::RecordRepositoryImpl, table::TableRepositoryImpl, view::ViewRepositoryImpl,
    },
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
    pub view: Arc<dyn ViewRepository>,
}

pub struct ConfigurationUseCases {
    pub get_active_colorscheme: UseCaseConfigurationGetActiveColorscheme,
}

pub struct UseCasesOperation {
    pub only_digits: UseCaseOnlyDigits,
}

pub struct UseCases {
    pub configuration: ConfigurationUseCases,
    pub operation: UseCasesOperation,
}

pub struct Injected {
    pub repository: Repositories,
    pub use_cases: UseCases,
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
            view: Arc::new(ViewRepositoryImpl::new(db.clone())),
        },
        use_cases: UseCases {
            configuration: ConfigurationUseCases {
                get_active_colorscheme: UseCaseConfigurationGetActiveColorscheme {},
            },
            operation: UseCasesOperation {
                only_digits: UseCaseOnlyDigits {},
            },
        },
    });

    services
}
