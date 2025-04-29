use crate::{
    domain::entities::configuration::Configuration,
    infrastructure::database::repositories::configuration::repository_configuration_get_inactive,
};

pub async fn provider_configuration_get_inactive() -> Vec<Configuration> {
    repository_configuration_get_inactive().await.unwrap()
}
