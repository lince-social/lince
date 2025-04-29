use crate::infrastructure::database::repositories::configuration::repository_configuration_set_active;

pub async fn provider_configuration_set_active(id: String) {
    repository_configuration_set_active(id).await
}
