pub mod providers;
pub mod use_cases;

use crate::infrastructure::cross_cutting::{
    providers::configuration::{ConfigurationProviders, injection_providers_configuration},
    use_cases::configuration::{ConfigurationUseCases, injection_use_cases_configuration},
};
use sqlx::{Pool, Sqlite};
use std::sync::Arc;

pub struct Providers {
    pub configuration: ConfigurationProviders,
}

pub struct UseCases {
    pub configuration: ConfigurationUseCases,
}

pub struct Injected {
    pub providers: Providers,
    pub use_cases: UseCases,
}

pub type InjectedServices = Arc<Injected>;

pub fn dependency_injection(db: Pool<Sqlite>) -> InjectedServices {
    let db = Arc::new(db);

    let services: InjectedServices = Arc::new(Injected {
        providers: Providers {
            configuration: injection_providers_configuration(db.clone()),
        },
        use_cases: UseCases {
            configuration: injection_use_cases_configuration(),
        },
    });

    services
}
