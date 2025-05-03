use crate::application::{
    providers::configuration::get_inactive::provider_configuration_get_inactive,
    schema::{configuration::row::ConfigurationForBarScheme, view::queried_view::QueriedView},
};

pub async fn use_case_configuration_get_inactive()
-> Vec<(ConfigurationForBarScheme, Vec<QueriedView>)> {
    provider_configuration_get_inactive().await
}
