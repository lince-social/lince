use crate::{
    application::schema::collection::row::ConfigurationRow,
    infrastructure::database::repositories::collection::repository_collection_get_inactive,
};

pub async fn provider_collection_get_inactive() -> Vec<ConfigurationRow> {
    repository_collection_get_inactive().await.unwrap()
}
