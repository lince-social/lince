use crate::{
    application::schema::configuration::row::ConfigurationRow,
    infrastructure::database::repositories::configuration::repository_configuration_get_inactive,
};

pub async fn provider_configuration_get_inactive() -> Vec<ConfigurationRow> {
    repository_configuration_get_inactive().await.unwrap()
}
