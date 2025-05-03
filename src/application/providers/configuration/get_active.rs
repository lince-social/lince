use crate::{
    application::schema::configuration::row::{ConfigurationForBarScheme, ConfigurationRow},
    infrastructure::database::repositories::configuration::repository_configuration_get_active,
};

pub async fn provider_configuration_get_active() -> ConfigurationRow {
    let (configuration, queried_views) = repository_configuration_get_active().await.unwrap();
    let configuration = ConfigurationForBarScheme::from(configuration);
    (configuration, queried_views)
}
