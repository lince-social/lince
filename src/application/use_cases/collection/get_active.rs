use crate::application::{
    providers::collection::get_active::provider_collection_get_active,
    schema::collection::row::ConfigurationRow,
};

pub async fn use_case_collection_get_active() -> ConfigurationRow {
    provider_collection_get_active().await
}
