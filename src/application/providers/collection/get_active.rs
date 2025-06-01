use crate::{
    application::schema::collection::row::ConfigurationRow,
    infrastructure::database::repositories::collection::repository_collection_get_active,
};

pub async fn provider_collection_get_active() -> ConfigurationRow {
    let (collection, queried_views) = repository_collection_get_active().await.unwrap();
    (collection, queried_views)
}
