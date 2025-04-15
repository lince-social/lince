use crate::{
    application::{
        providers::configuration::get_active::provider_configuration_get_active,
        schema::view::queried_view::QueriedView,
    },
    domain::entities::configuration::Configuration,
};

pub async fn use_case_configuration_get_active() -> (Configuration, Vec<QueriedView>) {
    provider_configuration_get_active().await
}
