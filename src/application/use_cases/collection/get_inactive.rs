use crate::{
    application::schema::{collection::row::ConfigurationForBarScheme, view::queried_view::QueriedView},
    infrastructure::cross_cutting::InjectedServices,
};

pub async fn use_case_collection_get_inactive(
    services: InjectedServices,
) -> Vec<(ConfigurationForBarScheme, Vec<QueriedView>)> {
    services.providers.collection.get_inactive().await
}
