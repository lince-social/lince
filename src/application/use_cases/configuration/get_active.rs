use crate::application::{
    providers::configuration::get_active::provider_configuration_get_active,
    schema::configuration::row::ConfigurationRow,
};

pub async fn use_case_configuration_get_active() -> ConfigurationRow {
    provider_configuration_get_active().await
}
