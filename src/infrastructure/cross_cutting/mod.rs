use crate::infrastructure::cross_cutting::providers::configuration::ConfigurationProviders;

pub mod providers;
pub mod use_cases;

struct Services {
    pub providers: Providers,
    pub use_cases: UseCases,
}

struct Providers {
    pub configuration: ConfigurationProviders,
}

struct UseCases {}
