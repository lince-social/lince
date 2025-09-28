use crate::{
    application::{
        providers::{
            collection::CollectionProvider, configuration::ConfigurationProvider,
            frequency::FrequencyProvider, karma::KarmaProvider, operation::OperationProvider,
            query::QueryProvider, record::RecordProvider, table::TableProvider, view::ViewProvider,
        },
        use_cases::{
            configuration::get_active_colorscheme::UseCaseConfigurationGetActiveColorscheme,
            operation::only_digits::UseCaseOnlyDigits,
        },
    },
    domain::repositories::command::CommandRepository,
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
    pub configuration: ConfigurationProvider,
    pub operation: OperationProvider,
    pub query: QueryProvider,
    pub record: RecordProvider,
    pub table: TableProvider,
    pub command: Arc<dyn CommandRepository>,
    pub frequency: FrequencyProvider,
    pub karma: KarmaProvider,
    pub collection: CollectionProvider,
    pub view: ViewProvider,
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
    pub repositories: Repositories,
    pub use_cases: UseCases,
}

pub type InjectedServices = Arc<Injected>;

pub fn dependency_injection(db: Arc<Pool<Sqlite>>) -> InjectedServices {
    let services: InjectedServices = Arc::new(Injected {
        repositories: Repositories {
            configuration: ConfigurationProvider {
                repository: Arc::new(ConfigurationRepositoryImpl::new(db.clone())),
            },
            operation: OperationProvider {
                repository: Arc::new(OperationRepositoryImpl::new(db.clone())),
            },
            query: QueryProvider {
                repository: Arc::new(QueryRepositoryImpl::new(db.clone())),
            },
            record: RecordProvider {
                repository: Arc::new(RecordRepositoryImpl::new(db.clone())),
            },
            table: TableProvider {
                repository: Arc::new(TableRepositoryImpl::new(db.clone())),
            },
            command: Arc::new(CommandRepositoryImpl::new(db.clone())),
            frequency: FrequencyProvider {
                repository: Arc::new(FrequencyRepositoryImpl::new(db.clone())),
            },
            karma: KarmaProvider {
                repository: Arc::new(KarmaRepositoryImpl::new(db.clone())),
            },
            collection: CollectionProvider {
                repository: Arc::new(CollectionRepositoryImpl::new(db.clone())),
            },
            view: ViewProvider {
                repository: Arc::new(ViewRepositoryImpl::new(db.clone())),
            },
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
