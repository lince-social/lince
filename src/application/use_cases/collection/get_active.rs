use crate::{
    application::schema::collection::row::ConfigurationRow,
    infrastructure::cross_cutting::InjectedServices,
};

pub async fn use_case_collection_get_active(services: InjectedServices) -> ConfigurationRow {
    services.providers.collection.get_active().await
}
