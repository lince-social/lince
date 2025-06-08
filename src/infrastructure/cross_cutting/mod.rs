use crate::{
    application::{
        providers::{
            command::CommandProvider, configuration::ConfigurationProvider,
            frequency::FrequencyProvider, karma::KarmaProvider, operation::OperationProvider,
            query::QueryProvider, record::RecordProvider, table::TableProvider,
        },
        use_cases::configuration::get_active_colorscheme::UseCaseConfigurationGetActiveColorscheme,
    },
    infrastructure::database::repositories::{
        command::CommandRepositoryImpl, configuration::ConfigurationRepositoryImpl,
        frequency::FrequencyRepositoryImpl, karma::KarmaRepositoryImpl,
        operation::OperationRepositoryImpl, query::QueryRepositoryImpl,
        record::RecordRepositoryImpl, table::TableRepositoryImpl,
    },
};
use sqlx::{Pool, Sqlite};
use std::sync::Arc;

pub struct Providers {
    pub configuration: ConfigurationProvider,
    pub operation: OperationProvider,
    pub query: QueryProvider,
    pub record: RecordProvider,
    pub table: TableProvider,
    pub command: CommandProvider,
    pub frequency: FrequencyProvider,
    pub karma: KarmaProvider,
}

pub struct UseCases {
    pub configuration: ConfigurationUseCases,
}

pub struct ConfigurationUseCases {
    pub get_active_colorscheme: UseCaseConfigurationGetActiveColorscheme,
}

pub struct Injected {
    pub providers: Providers,
    pub use_cases: UseCases,
}

pub type InjectedServices = Arc<Injected>;

pub fn dependency_injection(db: Pool<Sqlite>) -> InjectedServices {
    let db = Arc::new(db);

    let configuration_repository = Arc::new(ConfigurationRepositoryImpl::new(db.clone()));
    let operation_repository = Arc::new(OperationRepositoryImpl::new(db.clone()));
    let query_repository = Arc::new(QueryRepositoryImpl::new(db.clone()));
    let record_repository = Arc::new(RecordRepositoryImpl::new(db.clone()));
    let table_repository = Arc::new(TableRepositoryImpl::new(db.clone()));
    let command_repository = Arc::new(CommandRepositoryImpl::new(db.clone()));
    let frequency_repository = Arc::new(FrequencyRepositoryImpl::new(db.clone()));
    let karma_repository = Arc::new(KarmaRepositoryImpl::new(db.clone()));

    let services: InjectedServices = Arc::new(Injected {
        providers: Providers {
            configuration: ConfigurationProvider {
                repository: configuration_repository,
            },
            operation: OperationProvider {
                repository: operation_repository,
            },
            query: QueryProvider {
                repository: query_repository,
            },
            record: RecordProvider {
                repository: record_repository,
            },
            table: TableProvider {
                repository: table_repository,
            },
            command: CommandProvider {
                repository: command_repository,
            },
            frequency: FrequencyProvider {
                repository: frequency_repository,
            },
            karma: KarmaProvider {
                repository: karma_repository,
            },
        },
        use_cases: UseCases {
            configuration: ConfigurationUseCases {
                get_active_colorscheme: UseCaseConfigurationGetActiveColorscheme {},
            },
        },
    });

    services
}
