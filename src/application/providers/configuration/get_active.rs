use crate::{
    application::schema::view::queried_view::QueriedView,
    domain::entities::configuration::Configuration,
    infrastructure::database::repositories::configuration::repository_configuration_get_active,
};

pub async fn provider_configuration_get_active() -> (Configuration, Vec<QueriedView>) {
    repository_configuration_get_active().await.unwrap()
}
